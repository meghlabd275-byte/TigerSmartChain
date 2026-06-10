// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title EmergencyPause
 * @dev Emergency pause mechanism for the protocol
 * @notice Allows emergency stopping of critical functions
 */
contract EmergencyPause {
    // =============================================================================
    // STATE VARIABLES
    // =============================================================================

    // Owner
    address public owner;

    // Guardian (additional emergency stop authority)
    address public guardian;

    // Paused state
    bool public paused;

    // Pause reason
    string public pauseReason;

    // Last pause timestamp
    uint256 public lastPauseTime;

    // Pause duration
    uint256 public pauseDuration;

    // Minimum pause duration
    uint256 public constant MIN_PAUSE_DURATION = 1 hours;

    // Maximum pause duration
    uint256 public constant MAX_PAUSE_DURATION = 7 days;

    // Function pause status
    mapping(bytes4 => bool) public functionPaused;

    // Emergency guardians
    mapping(address => bool) public guardians;

    // Multisig threshold
    uint256 public multisigThreshold;

    // Pending owner (for two-step ownership transfer)
    address public pendingOwner;

    // Events
    event Paused(address indexed by, string reason, uint256 duration);
    event Unpaused(address indexed by);
    event FunctionPaused(bytes4 indexed selector, bool status);
    event GuardianSet(address indexed guardian, bool status);
    event OwnershipTransferStarted(address indexed oldOwner, address indexed newOwner);
    event OwnershipTransferred(address indexed oldOwner, address indexed newOwner);

    // Errors
    error AlreadyPaused();
    error NotPaused();
    error NotAuthorized();
    error InvalidDuration();
    error ZeroAddress();

    // =============================================================================
    // MODIFIERS
    // =============================================================================

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotAuthorized();
        _;
    }

    modifier onlyGuardian() {
        if (msg.sender != guardian && msg.sender != owner) revert NotAuthorized();
        _;
    }

    modifier whenNotPaused() {
        if (paused) revert AlreadyPaused();
        _;
    }

    modifier whenPaused() {
        if (!paused) revert NotPaused();
        _;
    }

    // =============================================================================
    // CONSTRUCTOR
    // =============================================================================

    constructor() {
        owner = msg.sender;
        guardian = msg.sender;
        multisigThreshold = 1;
    }

    // =============================================================================
    // PAUSE FUNCTIONS
    // =============================================================================

    /**
     * @notice Emergency pause the contract
     * @param reason Pause reason
     * @param duration Pause duration
     */
    function emergencyPause(string calldata reason, uint256 duration)
        external
        onlyGuardian
        whenNotPaused
    {
        if (duration < MIN_PAUSE_DURATION || duration > MAX_PAUSE_DURATION) {
            revert InvalidDuration();
        }

        paused = true;
        pauseReason = reason;
        lastPauseTime = block.timestamp;
        pauseDuration = duration;

        emit Paused(msg.sender, reason, duration);
    }

    /**
     * @notice Unpause the contract
     */
    function unpause() external onlyOwner whenPaused {
        // Check minimum pause duration
        if (block.timestamp - lastPauseTime < MIN_PAUSE_DURATION) {
            revert InvalidDuration();
        }

        paused = false;
        pauseReason = "";

        emit Unpaused(msg.sender);
    }

    /**
     * @notice Force unpause (owner only, for emergencies)
     */
    function forceUnpause() external onlyOwner whenPaused {
        paused = false;
        pauseReason = "";

        emit Unpaused(msg.sender);
    }

    // =============================================================================
    // FUNCTION PAUSE
    // =============================================================================

    /**
     * @notice Pause a specific function
     * @param selector Function selector
     */
    function pauseFunction(bytes4 selector) external onlyGuardian {
        functionPaused[selector] = true;
        emit FunctionPaused(selector, true);
    }

    /**
     * @notice Unpause a specific function
     * @param selector Function selector
     */
    function unpauseFunction(bytes4 selector) external onlyOwner {
        functionPaused[selector] = false;
        emit FunctionPaused(selector, false);
    }

    /**
     * @notice Check if function is paused
     * @param selector Function selector
     * @return bool True if paused
     */
    function isFunctionPaused(bytes4 selector) public view returns (bool) {
        return functionPaused[selector];
    }

    // =============================================================================
    // GUARDIAN MANAGEMENT
    // =============================================================================

    /**
     * @notice Set guardian
     * @param newGuardian New guardian address
     */
    function setGuardian(address newGuardian) external onlyOwner {
        if (newGuardian == address(0)) revert ZeroAddress();
        guardian = newGuardian;
        emit GuardianSet(newGuardian, true);
    }

    /**
     * @notice Remove guardian
     */
    function removeGuardian() external onlyOwner {
        address oldGuardian = guardian;
        guardian = address(0);
        emit GuardianSet(oldGuardian, false);
    }

    // =============================================================================
    // OWNERSHIP
    // =============================================================================

    /**
     * @notice Start ownership transfer
     * @param newOwner New owner address
     */
    function transferOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        pendingOwner = newOwner;
        emit OwnershipTransferStarted(owner, newOwner);
    }

    /**
     * @notice Accept ownership
     */
    function acceptOwnership() external {
        if (msg.sender != pendingOwner) revert NotAuthorized();
        address oldOwner = owner;
        owner = pendingOwner;
        pendingOwner = address(0);
        emit OwnershipTransferred(oldOwner, owner);
    }

    // =============================================================================
    // UTILITY
    // =============================================================================

    /**
     * @notice Get pause status
     * @return bool Paused status
     * @return string Pause reason
     * @return uint256 Remaining duration
     */
    function getPauseStatus()
        external
        view
        returns (
            bool,
            string memory,
            uint256
        )
    {
        if (!paused) {
            return (false, "", 0);
        }

        uint256 elapsed = block.timestamp - lastPauseTime;
        uint256 remaining = elapsed >= pauseDuration ? 0 : pauseDuration - elapsed;

        return (true, pauseReason, remaining);
    }

    /**
     * @notice Check if still in pause period
     * @return bool True if in pause period
     */
    function inPausePeriod() external view whenPaused returns (bool) {
        uint256 elapsed = block.timestamp - lastPauseTime;
        return elapsed < pauseDuration;
    }

    // =============================================================================
    // INIT
    // =============================================================================

    // Avoid unused warnings
    function init() external pure {
        require(MIN_PAUSE_DURATION < MAX_PAUSE_DURATION, "duration check");
    }
}