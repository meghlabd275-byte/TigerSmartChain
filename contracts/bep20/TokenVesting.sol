// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title TokenVesting
 * @dev Token vesting contract with cliff and linear vesting
 * @notice Manages locked tokens with vesting schedules
 */
contract TokenVesting {
    // =============================================================================
    // DATA STRUCTURES
    // =============================================================================

    // Vesting schedule
    struct VestingSchedule {
        address token;
        address beneficiary;
        uint256 totalAmount;
        uint256 startTime;
        uint256 cliffDuration;
        uint256 duration;
        uint256 released;
        bool revocable;
        bool revoked;
        bool immediateRelease;
    }

    // =============================================================================
    // STATE VARIABLES
    // =============================================================================

    // Owner
    address public owner;

    // Token
    address public token;

    // Vesting schedules
    mapping(bytes32 => VestingSchedule) public schedules;

    // Beneficiary schedules
    mapping(address => bytes32[]) public beneficiarySchedules;

    // Released amount
    mapping(address => uint256) public released;

    // Revoked count
    uint256 public revokedCount;

    // Events
    event VestingCreated(
        bytes32 indexed id,
        address indexed beneficiary,
        uint256 amount,
        uint256 startTime,
        uint256 duration
    );
    event TokenReleased(
        bytes32 indexed id,
        address indexed beneficiary,
        uint256 amount
    );
    event VestingRevoked(bytes32 indexed id);

    // Errors
    error Unauthorized();
    error InvalidAmount();
    error InvalidDuration();
    error ScheduleNotFound();
    error AlreadyRevoked();
    error NotRevocable();
    error ZeroAddress();
    error InvalidToken();

    // =============================================================================
    // MODIFIERS
    // =============================================================================

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    // =============================================================================
    // CONSTRUCTOR
    // =============================================================================

    constructor(address _token) {
        if (_token == address(0)) revert InvalidToken();
        owner = msg.sender;
        token = _token;
    }

    // =============================================================================
    // VESTING CREATION
    // =============================================================================

    /**
     * @notice Create a vesting schedule
     * @param beneficiary Beneficiary address
     * @param amount Total vesting amount
     * @param cliffDuration Cliff duration in seconds
     * @param duration Total vesting duration
     * @param revocable Whether revocable
     */
    function createVesting(
        address beneficiary,
        uint256 amount,
        uint256 cliffDuration,
        uint256 duration,
        bool revocable
    ) external onlyOwner returns (bytes32 scheduleId) {
        if (beneficiary == address(0)) revert ZeroAddress();
        if (amount == 0) revert InvalidAmount();
        if (duration == 0) revert InvalidDuration();

        // Create schedule ID
        scheduleId = keccak256(
            abi.encodePacked(beneficiary, amount, block.timestamp)
        );

        // Create schedule
        schedules[scheduleId] = VestingSchedule({
            token: token,
            beneficiary: beneficiary,
            totalAmount: amount,
            startTime: block.timestamp,
            cliffDuration: cliffDuration,
            duration: duration,
            released: 0,
            revocable: revocable,
            revoked: false,
            immediateRelease: false
        });

        // Add to beneficiary schedules
        beneficiarySchedules[beneficiary].push(scheduleId);

        // Transfer tokens (assumes token is already deposited)
        emit VestingCreated(
            scheduleId,
            beneficiary,
            amount,
            block.timestamp,
            duration
        );
    }

    /**
     * @notice Create vesting with immediate release
     * @param beneficiary Beneficiary address
     * @param amount Total amount
     * @param immediateAmount Immediate release amount
     * @param duration Vesting duration
     */
    function createVestingWithImmediate(
        address beneficiary,
        uint256 amount,
        uint256 immediateAmount,
        uint256 duration
    ) external onlyOwner returns (bytes32 scheduleId) {
        if (beneficiary == address(0)) revert ZeroAddress();
        if (amount == 0) revert InvalidAmount();
        if (duration == 0) revert InvalidDuration();
        if (immediateAmount > amount) revert InvalidAmount();

        scheduleId = keccak256(
            abi.encodePacked(beneficiary, amount, block.timestamp)
        );

        schedules[scheduleId] = VestingSchedule({
            token: token,
            beneficiary: beneficiary,
            totalAmount: amount,
            startTime: block.timestamp,
            cliffDuration: 0,
            duration: duration,
            released: immediateAmount,
            revocable: false,
            revoked: false,
            immediateRelease: true
        });

        beneficiarySchedules[beneficiary].push(scheduleId);

        // Transfer immediate release
        if (immediateAmount > 0) {
            _transferToken(beneficiary, immediateAmount);
            released[beneficiary] += immediateAmount;
        }

        emit VestingCreated(
            scheduleId,
            beneficiary,
            amount,
            block.timestamp,
            duration
        );
    }

    // =============================================================================
    // TOKEN RELEASE
    // =============================================================================

    /**
     * @notice Release vested tokens
     * @param scheduleId Vesting schedule ID
     */
    function release(bytes32 scheduleId) external {
        VestingSchedule storage schedule = schedules[scheduleId];
        
        if (schedule.totalAmount == 0) revert ScheduleNotFound();
        if (schedule.revoked) revert AlreadyRevoked();

        // Check beneficiary
        require(
            msg.sender == schedule.beneficiary || msg.sender == owner,
            "not beneficiary or owner"
        );

        // Calculate releasable amount
        uint256 releasable = _computeReleasable(schedule);
        if (releasable == 0) revert InvalidAmount();

        // Update released amount
        schedule.released += releasable;
        released[schedule.beneficiary] += releasable;

        // Transfer tokens
        _transferToken(schedule.beneficiary, releasable);

        emit TokenReleased(scheduleId, schedule.beneficiary, releasable);
    }

    /**
     * @notice Release all available for beneficiary
     * @param beneficiary Beneficiary address
     */
    function releaseAll(address beneficiary) external {
        bytes32[] storage scheduleIds = beneficiarySchedules[beneficiary];
        uint256 totalReleasable;

        for (uint256 i = 0; i < scheduleIds.length; i++) {
            VestingSchedule storage schedule = schedules[scheduleIds[i]];
            
            if (schedule.totalAmount == 0 || schedule.revoked) {
                continue;
            }

            // Check if caller is authorized
            require(
                msg.sender == schedule.beneficiary || msg.sender == owner,
                "not authorized"
            );

            uint256 releasable = _computeReleasable(schedule);
            if (releasable > 0) {
                schedule.released += releasable;
                totalReleasable += releasable;
            }
        }

        if (totalReleasable > 0) {
            released[beneficiary] += totalReleasable;
            _transferToken(beneficiary, totalReleasable);
            emit TokenReleased(
                keccak256(abi.encodePacked(beneficiary)),
                beneficiary,
                totalReleasable
            );
        }
    }

    // =============================================================================
    // VESTING REVOCATION
    // =============================================================================

    /**
     * @notice Revoke vesting schedule
     * @param scheduleId Vesting schedule ID
     */
    function revoke(bytes32 scheduleId) external onlyOwner {
        VestingSchedule storage schedule = schedules[scheduleId];
        
        if (schedule.totalAmount == 0) revert ScheduleNotFound();
        if (!schedule.revocable) revert NotRevocable();
        if (schedule.revoked) revert AlreadyRevoked();

        // Calculate releasable amount
        uint256 releasable = _computeReleasable(schedule);

        // Mark as revoked
        schedule.revoked = true;
        schedule.released += releasable;
        revokedCount++;

        // Release available tokens
        if (releasable > 0) {
            _transferToken(schedule.beneficiary, releasable);
        }

        emit VestingRevoked(scheduleId);
    }

    // =============================================================================
    // COMPUTATION FUNCTIONS
    // =============================================================================

    /**
     * @notice Compute releasable amount
     * @param schedule Vesting schedule
     * @return releasable Amount
     */
    function _computeReleasable(VestingSchedule storage schedule)
        internal
        returns (uint256)
    {
        uint256 vested = _computeVested(schedule);
        return vested - schedule.released;
    }

    /**
     * @notice Compute vested amount
     * @param schedule Vesting schedule
     * @return vested Vested amount
     */
    function _computeVested(VestingSchedule storage schedule)
        internal
        view
        returns (uint256)
    {
        uint256 total = schedule.totalAmount;
        uint256 start = schedule.startTime;
        uint256 cliff = schedule.cliffDuration;
        uint256 duration = schedule.duration;

        // Check cliff
        if (block.timestamp < start + cliff) {
            return 0;
        }

        // Check end
        if (block.timestamp >= start + duration) {
            return total;
        }

        // Linear vesting
        uint256 elapsed = block.timestamp - start;
        return (total * elapsed) / duration;
    }

    /**
     * @notice Transfer tokens
     * @param to Recipient
     * @param amount Amount
     */
    function _transferToken(address to, uint256 amount) internal {
        // Simplified - in production would use IERC20
        (to, amount);
    }

    // =============================================================================
    // QUERY FUNCTIONS
    // =============================================================================

    /**
     * @notice Get releasable amount for schedule
     * @param scheduleId Schedule ID
     * @return releasable Amount
     */
    function getReleasableAmount(bytes32 scheduleId)
        external
        view
        returns (uint256)
    {
        VestingSchedule storage schedule = schedules[scheduleId];
        
        if (schedule.totalAmount == 0 || schedule.revoked) {
            return 0;
        }

        uint256 vested = _computeVested(schedule);
        return vested - schedule.released;
    }

    /**
     * @notice Get vested amount for schedule
     * @param scheduleId Schedule ID
     * @return vested Amount
     */
    function getVestedAmount(bytes32 scheduleId)
        external
        view
        returns (uint256)
    {
        VestingSchedule storage schedule = schedules[scheduleId];
        
        if (schedule.totalAmount == 0) {
            return 0;
        }

        return _computeVested(schedule);
    }

    /**
     * @notice Get schedules for beneficiary
     * @param beneficiary Beneficiary address
     * @return Array of schedule IDs
     */
    function getSchedules(address beneficiary)
        external
        view
        returns (bytes32[] memory)
    {
        return beneficiarySchedules[beneficiary];
    }

    /**
     * @notice Get schedule details
     * @param scheduleId Schedule ID
     * @return VestingSchedule
     */
    function getScheduleDetails(bytes32 scheduleId)
        external
        view
        returns (VestingSchedule memory)
    {
        return schedules[scheduleId];
    }

    // =============================================================================
    // ADMIN
    // =============================================================================

    /**
     * @notice Set owner
     * @param newOwner New owner address
     */
    function setOwner(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        owner = newOwner;
    }

    /**
     * @notice Set token
     * @param newToken New token address
     */
    function setToken(address newToken) external onlyOwner {
        if (newToken == address(0)) revert InvalidToken();
        token = newToken;
    }
}