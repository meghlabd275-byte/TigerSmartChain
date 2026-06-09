// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Token Vesting
/// @notice Token vesting with linear release and cliff
/// @dev Supports multiple beneficiaries and revocable vests

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/Address.sol";

/// @title Vesting Schedule
/// @dev Represents a vesting schedule
struct VestingSchedule {
    address token;
    address beneficiary;
    uint256 totalAmount;
    uint256 startTime;
    uint256 cliffDuration;
    uint256 duration;
    uint256 releasedAmount;
    bool revocable;
    bool revoked;
}

/// @title Token Vesting Contract
/// @dev Linear vesting with optional cliff
contract TokenVesting is Ownable {
    using SafeERC20 for IERC20;

    // Duration unit in seconds
    uint256 public constant DURATION_UNIT = 1 seconds;

    // Maximum vesting duration (5 years)
    uint256 public constant MAX_DURATION = 5 * 365 days;

    // Minimum cliff period (1 day)
    uint256 public constant MIN_CLIFF = 1 days;

    // Vesting schedules
    mapping(bytes32 => VestingSchedule) public vestingSchedules;

    // Beneficiary to schedule IDs
    mapping(address => bytes32[]) public beneficiarySchedules;

    // Released events
    event Released(address indexed beneficiary, uint256 amount);
    event Revoked(address indexed beneficiary);

    /// @notice Creates a vesting schedule
    /// @param token Token to vest
    /// @param beneficiary Beneficiary address
    /// @param totalAmount Total amount to vest
    /// @param startTime Start timestamp (0 for current time)
    /// @param cliffDuration Cliff period in seconds
    /// @param duration Total vesting duration
    /// @param revocable Whether the vesting is revocable
    function createVesting(
        address token,
        address beneficiary,
        uint256 totalAmount,
        uint256 startTime,
        uint256 cliffDuration,
        uint256 duration,
        bool revocable
    ) external {
        require(beneficiary != address(0), "Invalid beneficiary");
        require(totalAmount > 0, "Amount is 0");
        require(duration > 0 && duration <= MAX_DURATION, "Invalid duration");
        require(cliffDuration <= duration, "Cliff exceeds duration");
        require(startTime >= block.timestamp || startTime == 0, "Invalid start time");

        // Transfer tokens to contract
        IERC20(token).safeTransferFrom(msg.sender, address(this), totalAmount);

        // Create schedule
        bytes32 scheduleId = keccak256(
            abi.encodePacked(token, beneficiary, block.timestamp)
        );

        vestingSchedules[scheduleId] = VestingSchedule({
            token: token,
            beneficiary: beneficiary,
            totalAmount: totalAmount,
            startTime: startTime == 0 ? block.timestamp : startTime,
            cliffDuration: cliffDuration,
            duration: duration,
            releasedAmount: 0,
            revocable: revocable,
            revoked: false
        });

        beneficiarySchedules[beneficiary].push(scheduleId);
    }

    /// @notice Releases vested tokens
    function release(bytes32 scheduleId) external {
        VestingSchedule storage schedule = vestingSchedules[scheduleId];
        require(schedule.beneficiary == msg.sender, "Not beneficiary");

        uint256 releasable = computeReleasable(scheduleId);
        require(releasable > 0, "No tokens to release");

        schedule.releasedAmount += releasable;

        IERC20(schedule.token).safeTransfer(
            schedule.beneficiary,
            releasable
        );

        emit Released(schedule.beneficiary, releasable);
    }

    /// @notice Computes releasable amount
    function computeReleasable(bytes32 scheduleId)
        public
        view
        returns (uint256)
    {
        VestingSchedule memory schedule = vestingSchedules[scheduleId];

        if (schedule.revoked || block.timestamp < schedule.startTime) {
            return 0;
        }

        uint256 timeSinceStart = block.timestamp - schedule.startTime;

        // Before cliff
        if (timeSinceStart < schedule.cliffDuration) {
            return 0;
        }

        // After full duration
        if (timeSinceStart >= schedule.duration) {
            return schedule.totalAmount - schedule.releasedAmount;
        }

        // During vesting
        uint256 vestedAmount = (schedule.totalAmount * timeSinceStart) /
            schedule.duration;

        return vestedAmount - schedule.releasedAmount;
    }

    /// @notice Revokes a vesting
    function revoke(bytes32 scheduleId) external onlyOwner {
        VestingSchedule storage schedule = vestingSchedules[scheduleId];
        require(schedule.revocable, "Not revocable");
        require(!schedule.revoked, "Already revoked");

        // Release releasable first
        uint256 releasable = computeReleasable(scheduleId);
        if (releasable > 0) {
            schedule.releasedAmount += releasable;
            IERC20(schedule.token).safeTransfer(
                schedule.beneficiary,
                releasable
            );
        }

        // Mark as revoked
        schedule.revoked = true;

        // Return unvested tokens to owner
        uint256 remaining = schedule.totalAmount - schedule.releasedAmount;
        if (remaining > 0) {
            IERC20(schedule.token).safeTransfer(owner(), remaining);
        }

        emit Revoked(schedule.beneficiary);
    }

    /// @notice Returns vesting schedule
    function getVestingSchedule(bytes32 scheduleId)
        external
        view
        returns (
            address token,
            address beneficiary,
            uint256 totalAmount,
            uint256 releasedAmount,
            bool revocable,
            bool revoked
        )
    {
        VestingSchedule memory schedule = vestingSchedules[scheduleId];
        return (
            schedule.token,
            schedule.beneficiary,
            schedule.totalAmount,
            schedule.releasedAmount,
            schedule.revocable,
            schedule.revoked
        );
    }

    /// @notice Returns schedules for beneficiary
    function getBeneficiarySchedules(address beneficiary)
        external
        view
        returns (bytes32[] memory)
    {
        return beneficiarySchedules[beneficiary];
    }
}

/// @title Token Timelock
/// @dev Time-locked token release
contract TokenTimelock is Ownable {
    using SafeERC20 for IERC20;

    // Token to release
    IERC20 public immutable token;

    // Release timestamp
    uint256 public releaseTime;

    // Released flag
    bool public released;

    // Event
    event Released(uint256 amount);

    constructor(address _token, uint256 _releaseTime) {
        require(_token != address(0), "Invalid token");
        require(_releaseTime > block.timestamp, "Invalid release time");

        token = IERC20(_token);
        releaseTime = _releaseTime;
    }

    /// @notice Releases tokens
    function release() external onlyOwner {
        require(!released, "Already released");
        require(block.timestamp >= releaseTime, "Not yet released");

        released = true;

        uint256 balance = token.balanceOf(address(this));
        require(balance > 0, "No tokens to release");

        token.safeTransfer(owner(), balance);

        emit Released(balance);
    }

    /// @notice Returns time until release
    function timeUntilRelease() external view returns (uint256) {
        if (block.timestamp >= releaseTime) {
            return 0;
        }
        return releaseTime - block.timestamp;
    }
}

/// @title Multi-stage Vesting
/// @dev Vesting with multiple release stages
contract MultiStageVesting is Ownable {
    using SafeERC20 for IERC20;

    // Stage structure
    struct Stage {
        uint256 releaseTime;
        uint256 percentage; // 0-10000 = 0-100%
    }

    // Stages
    Stage[] public stages;

    // Beneficiary
    address public beneficiary;

    // Token
    IERC20 public token;

    // Total amount
    uint256 public totalAmount;

    // Released amount
    uint256 public releasedAmount;

    // Stage release events
    event StageReleased(uint256 stageIndex, uint256 amount);

    constructor(address _token, address _beneficiary) {
        require(_token != address(0), "Invalid token");
        require(_beneficiary != address(0), "Invalid beneficiary");

        token = IERC20(_token);
        beneficiary = _beneficiary;
    }

    /// @notice Adds a release stage
    function addStage(uint256 releaseTime, uint256 percentage)
        external
        onlyOwner
    {
        require(percentage <= 10000, "Invalid percentage");
        require(releaseTime > block.timestamp, "Invalid release time");

        // Check total doesn't exceed 100%
        uint256 total;
        for (uint256 i = 0; i < stages.length; i++) {
            total += stages[i].percentage;
        }
        require(total + percentage <= 10000, "Exceeds 100%");

        stages.push(Stage({
            releaseTime: releaseTime,
            percentage: percentage
        }));
    }

    /// @notice Sets total amount (called after funding)
    function setTotalAmount(uint256 amount) external onlyOwner {
        require(totalAmount == 0, "Already set");
        require(amount > 0, "Invalid amount");

        totalAmount = amount;
    }

    /// @notice Releases available tokens
    function release() external {
        require(msg.sender == beneficiary || msg.sender == owner(), "Not authorized");

        uint256 available = computeReleasable();
        require(available > 0, "Nothing to release");

        releasedAmount += available;
        token.safeTransfer(beneficiary, available);

        emit StageReleased(stages.length, available);
    }

    /// @notice Computes releasable amount
    function computeReleasable() public view returns (uint256) {
        uint256 total;
        for (uint256 i = 0; i < stages.length; i++) {
            if (block.timestamp >= stages[i].releaseTime) {
                total += (totalAmount * stages[i].percentage) / 10000;
            }
        }

        return total - releasedAmount;
    }

    /// @notice Returns stage count
    function stageCount() external view returns (uint256) {
        return stages.length;
    }
}