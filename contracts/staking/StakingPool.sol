// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title TEP20 Staking Contract
/// @notice Staking contract for TigerSmartChain

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";

/// @notice Staking Pool Contract
contract StakingPool is ReentrancyGuard, AccessControl {
    using SafeERC20 for IERC20;
    
    /// @notice Staking token
    IERC20 public immutable stakingToken;
    
    /// @notice Reward token
    IERC20 public immutable rewardToken;
    
    /// @notice Duration of rewards to be paid out (in seconds)
    uint256 public immutable rewardsDuration;
    
    /// @notice Last update time
    uint256 public lastUpdateTime;
    
    /// @notice Reward per token stored
    uint256 public rewardPerTokenStored;
    
    /// @notice Reward rate (rewards per second)
    uint256 public rewardRate;
    
    /// @notice Total staked
    uint256 public totalStaked;
    
    /// @notice Minimum stake amount
    uint256 public minStakeAmount;
    
    /// @notice Maximum stake amount
    uint256 public maxStakeAmount;
    
    /// @notice Reward pool
    uint256 public rewardPool;
    
    /// @notice Staker data
    struct StakerData {
        uint256 staked;
        uint256 rewards;
        uint256 rewardPerTokenPaid;
        uint64 stakeTime;
        uint64 unlockTime;
        bool withdrawn;
    }
    
    /// @notice Stakers mapping
    mapping(address => StakerData) public stakers;
    
    /// @notice Lock period (in seconds)
    uint64 public lockPeriod = 90 days;
    
    /// @notice Early withdrawal penalty (in basis points)
    uint256 public earlyPenalty = 500; // 5%
    
    /// @notice Events
    event Staked(address indexed user, uint256 amount);
    event Unstaked(address indexed user, uint256 amount, uint256 penalty);
    event RewardClaimed(address indexed user, uint256 reward);
    event RewardRateUpdated(uint256 newRate);
    event RewardPoolUpdated(uint256 newPool);
    event LockPeriodUpdated(uint64 newPeriod);
    event EarlyPenaltyUpdated(uint256 newPenalty);
    
    /// @notice Constructor
    /// @param _stakingToken Staking token address
    /// @param _rewardToken Reward token address
    /// @param _rewardsDuration Rewards duration
    constructor(
        address _stakingToken,
        address _rewardToken,
        uint256 _rewardsDuration
    ) {
        require(_stakingToken != address(0), "invalid staking token");
        require(_rewardToken != address(0), "invalid reward token");
        
        stakingToken = IERC20(_stakingToken);
        rewardToken = IERC20(_rewardToken);
        rewardsDuration = _rewardsDuration;
        
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        
        minStakeAmount = 100 * 10**18;
    }
    
    /// @notice Stake tokens
    function stake(uint256 amount) external nonReentrant {
        require(amount >= minStakeAmount, "amount below minimum");
        require(
            maxStakeAmount == 0 || stakers[msg.sender].staked + amount <= maxStakeAmount,
            "amount exceeds maximum"
        );
        
        // Update reward
        updateReward(msg.sender);
        
        // Transfer tokens
        stakingToken.safeTransferFrom(msg.sender, address(this), amount);
        
        // Update staker
        StakerData storage staker = stakers[msg.sender];
        staker.staked += amount;
        if (staker.stakeTime == 0) {
            staker.stakeTime = uint64(block.timestamp);
        }
        staker.unlockTime = uint64(block.timestamp + lockPeriod);
        
        totalStaked += amount;
        
        emit Staked(msg.sender, amount);
    }
    
    /// @notice Unstake tokens
    function unstake(uint256 amount) external nonReentrant {
        StakerData storage staker = stakers[msg.sender];
        require(staker.staked >= amount, "insufficient stake");
        
        // Update reward
        updateReward(msg.sender);
        
        // Check lock period
        uint256 penalty = 0;
        if (block.timestamp < staker.unlockTime) {
            penalty = (amount * earlyPenalty) / 10000;
            // Penalty goes to treasury
        }
        
        // Update staker
        staker.staked -= amount;
        if (staker.staked == 0) {
            staker.stakeTime = 0;
            staker.unlockTime = 0;
            staker.withdrawn = true;
        }
        
        totalStaked -= amount;
        
        // Transfer tokens
        stakingToken.safeTransfer(msg.sender, amount - penalty);
        
        emit Unstaked(msg.sender, amount, penalty);
    }
    
    /// @notice Claim rewards
    function claimReward() external nonReentrant {
        updateReward(msg.sender);
        
        StakerData storage staker = stakers[msg.sender];
        uint256 reward = staker.rewards;
        
        require(reward > 0, "no rewards");
        
        staker.rewards = 0;
        
        rewardToken.safeTransfer(msg.sender, reward);
        
        emit RewardClaimed(msg.sender, reward);
    }
    
    /// @notice Set reward rate
    function setRewardRate(uint256 _rewardRate) external onlyRole(DEFAULT_ADMIN_ROLE) {
        updateReward(address(0));
        
        rewardRate = _rewardRate;
        
        emit RewardRateUpdated(_rewardRate);
    }
    
    /// @notice Add rewards to pool
    function addRewards(uint256 amount) external onlyRole(DEFAULT_ADMIN_ROLE) {
        rewardToken.safeTransferFrom(msg.sender, address(this), amount);
        
        rewardPool += amount;
        
        // Update reward rate
        if (rewardsDuration > 0) {
            rewardRate = rewardPool / rewardsDuration;
            rewardPool = rewardPool - (rewardRate * rewardsDuration);
        }
        
        emit RewardPoolUpdated(amount);
    }
    
    /// @notice Set lock period
    function setLockPeriod(uint64 _lockPeriod) external onlyRole(DEFAULT_ADMIN_ROLE) {
        lockPeriod = _lockPeriod;
        
        emit LockPeriodUpdated(_lockPeriod);
    }
    
    /// @notice Set early penalty
    function setEarlyPenalty(uint256 _earlyPenalty) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(_earlyPenalty <= 2000, "penalty too high");
        
        earlyPenalty = _earlyPenalty;
        
        emit EarlyPenaltyUpdated(_earlyPenalty);
    }
    
    /// @notice Set min/max stake
    function setStakeLimits(uint256 _minStake, uint256 _maxStake) external onlyRole(DEFAULT_ADMIN_ROLE) {
        minStakeAmount = _minStake;
        maxStakeAmount = _maxStake;
    }
    
    /// @notice Update reward for account
    function updateReward(address account) internal {
        rewardPerTokenStored = rewardPerToken();
        lastUpdateTime = block.timestamp;
        
        if (account != address(0)) {
            StakerData storage staker = stakers[account];
            staker.rewards = earned(account);
            staker.rewardPerTokenPaid = rewardPerTokenStored;
        }
    }
    
    /// @notice Calculate reward per token
    function rewardPerToken() internal view returns (uint256) {
        if (totalStaked == 0) {
            return rewardPerTokenStored;
        }
        
        return rewardPerTokenStored + (
            (block.timestamp - lastUpdateTime) * rewardRate * 1e18 / totalStaked
        );
    }
    
    /// @notice Calculate earned rewards
    function earned(address account) internal view returns (uint256) {
        StakerData storage staker = stakers[account];
        
        return staker.staked * (rewardPerToken() - staker.rewardPerTokenPaid) / 1e18 
            + staker.rewards;
    }
    
    /// @notice View earned rewards
    function earnedRewards(address account) external view returns (uint256) {
        return earned(account);
    }
    
    /// @notice View staker info
    function stakerInfo(address account) external view returns (
        uint256 staked,
        uint256 rewards,
        uint64 stakeTime,
        uint64 unlockTime,
        bool canUnstake
    ) {
        StakerData storage staker = stakers[account];
        
        return (
            staker.staked,
            staker.rewards,
            staker.stakeTime,
            staker.unlockTime,
            block.timestamp >= staker.unlockTime
        );
    }
}