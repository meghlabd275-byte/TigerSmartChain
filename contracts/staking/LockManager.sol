// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title LockManager
 * @dev Token lock manager for staking and vesting
 * @notice Manages locked tokens for validators and delegators
 */
contract LockManager {
    // =============================================================================
    // DATA STRUCTURES
    // =============================================================================

    // Lock structure
    struct Lock {
        address beneficiary;
        uint256 amount;
        uint256 startTime;
        uint256 endTime;
        uint256 cliffDuration;
        uint256 released;
        bool revocable;
        bool revoked;
    }

    // Lock schedule
    enum LockType {
        Immediate,
        Linear,
        Cliff,
        Custom
    }

    // =============================================================================
    // STATE VARIABLES
    // =============================================================================

    // Token contract
    address public token;

    // Owner
    address public owner;

    // Locks by ID
    mapping(bytes32 => Lock) public locks;

    // Beneficiary locks (address => lock IDs)
    mapping(address => bytes32[]) public beneficiaryLocks;

    // Total locked
    uint256 public totalLocked;

    // Total unlocked
    uint256 public totalUnlocked;

    // Minimum lock amount
    uint256 public minLockAmount;

    // Maximum lock duration
    uint256 public maxLockDuration;

    // Default cliff period
    uint256 public defaultCliffPeriod;

    // Lock count
    uint256 public lockCount;

    // Emergency stop
    bool public stopped;

    // Events
    event LockCreated(
        bytes32 indexed lockId,
        address indexed beneficiary,
        uint256 amount,
        uint256 endTime
    );
    event LockReleased(
        bytes32 indexed lockId,
        address indexed beneficiary,
        uint256 amount
    );
    event LockRevoked(bytes32 indexed lockId);
    event TokenWithdrawn(address indexed to, uint256 amount);
    event EmergencyStop(bool stopped);

    // Errors
    error Unauthorized();
    error InvalidAmount();
    error InvalidDuration();
    error InvalidBeneficiary();
    error LockNotFound();
    error LockRevoked();
    error LockNotRevocable();
    error StillLocked();
    error ZeroAddress();
    error ContractStopped();

    // =============================================================================
    // MODIFIERS
    // =============================================================================

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    modifier notStopped() {
        if (stopped) revert ContractStopped();
        _;
    }

    // =============================================================================
    // CONSTRUCTOR
    // =============================================================================

    constructor(address _token) {
        require(_token != address(0), ZeroAddress());
        token = _token;
        owner = msg.sender;
        minLockAmount = 1 ether;
        maxLockDuration = 365 days * 5; // 5 years
        defaultCliffPeriod = 90 days;
    }

    // =============================================================================
    // LOCK CREATION
    // =============================================================================

    /**
     * @notice Create a new lock
     * @param beneficiary Beneficiary address
     * @param amount Lock amount
     * @param duration Lock duration
     * @param lockType Lock type
     * @return lockId Lock ID
     */
    function createLock(
        address beneficiary,
        uint256 amount,
        uint256 duration,
        LockType lockType
    ) external onlyOwner notStopped returns (bytes32 lockId) {
        if (beneficiary == address(0)) revert InvalidBeneficiary();
        if (amount < minLockAmount) revert InvalidAmount();
        if (duration < 1 days || duration > maxLockDuration) revert InvalidDuration();

        // Calculate cliff
        uint256 cliffDuration;
        if (lockType == LockType.Cliff) {
            cliffDuration = defaultCliffPeriod;
        } else {
            cliffDuration = 0;
        }

        // Generate lock ID
        lockId = keccak256(
            abi.encodePacked(beneficiary, amount, block.timestamp, lockCount);
        lockCount++;

        // Create lock
        locks[lockId] = Lock({
            beneficiary: beneficiary,
            amount: amount,
            startTime: block.timestamp,
            endTime: block.timestamp + duration,
            cliffDuration: cliffDuration,
            released: 0,
            revocable: true,
            revoked: false
        });

        // Add to beneficiary locks
        beneficiaryLocks[beneficiary].push(lockId);

        // Update total locked
        totalLocked += amount;

        emit LockCreated(lockId, beneficiary, amount, block.timestamp + duration);
    }

    /**
     * @notice Create multiple locks (batch)
     * @param beneficiaries Array of beneficiary addresses
     * @param amounts Array of lock amounts
     * @param durations Array of lock durations
     * @return Array of lock IDs
     */
    function createMultipleLocks(
        address[] calldata beneficiaries,
        uint256[] calldata amounts,
        uint256[] calldata durations
    ) external onlyOwner notStopped returns (bytes32[] memory lockIds) {
        require(
            beneficiaries.length == amounts.length &&
                beneficiaries.length == durations.length,
            "Length mismatch"
        );

        lockIds = new bytes32[](beneficiaries.length);

        for (uint256 i = 0; i < beneficiaries.length; i++) {
            lockIds[i] = createLock(
                beneficiaries[i],
                amounts[i],
                durations[i],
                LockType.Linear
            );
        }
    }

    // =============================================================================
    // LOCK RELEASE
    // =============================================================================

    /**
     * @notice Release available tokens
     * @param lockId Lock ID
     * @return released Amount released
     */
    function release(bytes32 lockId) external notStopped returns (uint256 released) {
        Lock storage lock = locks[lockId];
        if (lock.amount == 0) revert LockNotFound();
        if (lock.revoked) revert LockRevoked();

        // Calculate releasable amount
        released = _calculateReleasable(lock);
        if (released == 0) revert StillLocked();

        // Update released amount
        lock.released += released;
        totalReleased += released;
        totalLocked -= released;

        emit LockReleased(lockId, lock.beneficiary, released);
    }

    /**
     * @notice Calculate releasable amount
     */
    function _calculateReleasable(Lock storage lock)
        internal
        returns (uint256 releasable)
    {
        // Check cliff
        if (
            lock.cliffDuration > 0 &&
            block.timestamp < lock.startTime + lock.cliffDuration
        ) {
            return 0;
        }

        // Calculate vested amount based on lock type
        uint256 vested = _calculateVested(lock);
        releasable = vested - lock.released;
    }

    /**
     * @notice Calculate vested amount
     */
    function _calculateVested(Lock storage lock)
        internal
        view
        returns (uint256 vested)
    {
        uint256 elapsed = block.timestamp - lock.startTime;
        uint256 totalDuration = lock.endTime - lock.startTime;

        if (elapsed >= totalDuration) {
            // Full vesting
            return lock.amount;
        }

        // Linear vesting
        vested = (lock.amount * elapsed) / totalDuration;
    }

    // State variable for total released
    uint256 public totalReleased;

    // =============================================================================
    // LOCK REVOCATION
    // =============================================================================

    /**
     * @notice Revoke a lock
     * @param lockId Lock ID
     */
    function revokeLock(bytes32 lockId) external onlyOwner notStopped {
        Lock storage lock = locks[lockId];
        if (lock.amount == 0) revert LockNotFound();
        if (!lock.revocable) revert LockNotRevocable();
        if (lock.revoked) revert LockRevoked();

        // Calculate releasable amount
        uint256 releasable = _calculateReleasable(lock);

        // Mark as revoked
        lock.revoked = true;
        lock.released += releasable;

        // Update totals
        totalReleased += releasable;
        totalLocked -= (lock.amount - lock.released);

        emit LockRevoked(lockId);
    }

    // =============================================================================
    // QUERY FUNCTIONS
    // =============================================================================

    /**
     * @notice Get lock details
     * @param lockId Lock ID
     * @return Lock struct
     */
    function getLock(bytes32 lockId) external view returns (Lock memory) {
        return locks[lockId];
    }

    /**
     * @notice Get locks for beneficiary
     * @param beneficiary Beneficiary address
     * @return Array of lock IDs
     */
    function getLocksForBeneficiary(address beneficiary)
        external
        view
        returns (bytes32[] memory)
    {
        return beneficiaryLocks[beneficiary];
    }

    /**
     * @notice Get releasable amount
     * @param lockId Lock ID
     * @return releasable Amount that can be released
     */
    function getReleasableAmount(bytes32 lockId)
        external
        view
        returns (uint256 releasable)
    {
        Lock storage lock = locks[lockId];
        if (lock.amount == 0 || lock.revoked) {
            return 0;
        }
        return _calculateReleasable(lock);
    }

    /**
     * @notice Get locked amount for beneficiary
     * @param beneficiary Beneficiary address
     * @return total Total locked amount
     */
    function getLockedAmount(address beneficiary)
        external
        view
        returns (uint256 total)
    {
        bytes32[] memory lockIds = beneficiaryLocks[beneficiary];
        for (uint256 i = 0; i < lockIds.length; i++) {
            Lock storage lock = locks[lockIds[i]];
            if (!lock.revoked) {
                total += (lock.amount - lock.released);
            }
        }
    }

    // =============================================================================
    // ADMIN FUNCTIONS
    // =============================================================================

    /**
     * @notice Set minimum lock amount
     * @param amount New minimum
     */
    function setMinLockAmount(uint256 amount) external onlyOwner {
        minLockAmount = amount;
    }

    /**
     * @notice Set maximum lock duration
     * @param duration New maximum
     */
    function setMaxLockDuration(uint256 duration) external onlyOwner {
        maxLockDuration = duration;
    }

    /**
     * @notice Set default cliff period
     * @param cliff New cliff period
     */
    function setDefaultCliffPeriod(uint256 cliff) external onlyOwner {
        defaultCliffPeriod = cliff;
    }

    /**
     * @notice Emergency stop
     * @param _stopped Stop status
     */
    function emergencyStop(bool _stopped) external onlyOwner {
        stopped = _stopped;
        emit EmergencyStop(_stopped);
    }

    /**
     * @notice Transfer ownership
     * @param newOwner New owner
     */
    function transferOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        owner = newOwner;
    }

    // =============================================================================
    // INIT
    // =============================================================================

    // Avoid unused warnings
    function init() external pure {
        require(maxLockDuration > 0, "duration check");
    }
}