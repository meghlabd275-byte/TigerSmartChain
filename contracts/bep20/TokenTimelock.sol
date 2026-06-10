// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title TokenTimelock
 * @dev Token timelock for time-locked token releases
 * @notice Locks tokens and releases after a specific time
 */
contract TokenTimelock {
    // =============================================================================
    // DATA STRUCTURES
    // =============================================================================

    // Timelock schedule
    struct Timelock {
        address token;
        address beneficiary;
        uint256 amount;
        uint256 releaseTime;
        uint256 released;
        bool revoked;
    }

    // =============================================================================
    // STATE VARIABLES
    // =============================================================================

    // Owner
    address public owner;

    // Token
    address public token;

    // Timelocks
    mapping(bytes32 => Timelock) public timelocks;

    // Beneficiary timelocks
    mapping(address => bytes32[]) public beneficiaryTimelocks;

    // Total locked
    uint256 public totalLocked;

    // Released total
    uint256 public totalReleased;

    // Events
    event TokenLocked(
        bytes32 indexed id,
        address indexed beneficiary,
        uint256 amount,
        uint256 releaseTime
    );
    event TokenReleased(
        bytes32 indexed id,
        address indexed beneficiary,
        uint256 amount
    );
    event TimelockRevoked(bytes32 indexed id);

    // Errors
    error Unauthorized();
    error InvalidAmount();
    error InvalidTime();
    error NotYetReleased();
    error AlreadyRevoked();
    error ZeroAddress();

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
        if (_token == address(0)) revert Unauthorized();
        owner = msg.sender;
        token = _token;
    }

    // =============================================================================
    // LOCKING
    // =============================================================================

    /**
     * @notice Lock tokens
     * @param beneficiary Beneficiary address
     * @param amount Amount to lock
     * @param releaseTime Release timestamp
     * @return timelockId Timelock ID
     */
    function lock(
        address beneficiary,
        uint256 amount,
        uint256 releaseTime
    ) external onlyOwner returns (bytes32 timelockId) {
        if (beneficiary == address(0)) revert ZeroAddress();
        if (amount == 0) revert InvalidAmount();
        if (releaseTime <= block.timestamp) revert InvalidTime();

        // Create timelock ID
        timelockId = keccak256(
            abi.encodePacked(beneficiary, amount, releaseTime, block.timestamp)
        );

        // Create timelock
        timelocks[timelockId] = Timelock({
            token: token,
            beneficiary: beneficiary,
            amount: amount,
            releaseTime: releaseTime,
            released: 0,
            revoked: false
        });

        // Add to beneficiary timelocks
        beneficiaryTimelocks[beneficiary].push(timelockId);

        // Update total
        totalLocked += amount;

        emit TokenLocked(timelockId, beneficiary, amount, releaseTime);
    }

    /**
     * @notice Lock multiple beneficiaries
     * @param beneficiaries Array of beneficiary addresses
     * @param amounts Array of amounts
     * @param releaseTime Release timestamp
     * @return Array of timelock IDs
     */
    function lockMultiple(
        address[] calldata beneficiaries,
        uint256[] calldata amounts,
        uint256 releaseTime
    ) external onlyOwner returns (bytes32[] memory timelockIds) {
        require(
            beneficiaries.length == amounts.length,
            "length mismatch"
        );
        require(releaseTime > block.timestamp, "invalid time");

        timelockIds = new bytes32[](beneficiaries.length);

        for (uint256 i = 0; i < beneficiaries.length; i++) {
            timelockIds[i] = lock(
                beneficiaries[i],
                amounts[i],
                releaseTime
            );
        }
    }

    // =============================================================================
    // RELEASE
    // =============================================================================

    /**
     * @notice Release locked tokens
     * @param timelockId Timelock ID
     */
    function release(bytes32 timelockId) external {
        Timelock storage timelock = timelocks[timelockId];
        
        if (timelock.amount == 0) revert Unauthorized();
        if (timelock.revoked) revert AlreadyRevoked();
        if (block.timestamp < timelock.releaseTime) revert NotYetReleased();

        // Check beneficiary
        require(
            msg.sender == timelock.beneficiary || msg.sender == owner,
            "not beneficiary or owner"
        );

        // Calculate release amount
        uint256 releaseAmount = timelock.amount - timelock.released;
        if (releaseAmount == 0) revert InvalidAmount();

        // Update timelock
        timelock.released += releaseAmount;
        totalReleased += releaseAmount;
        totalLocked -= releaseAmount;

        // Transfer tokens
        _transferToken(timelock.beneficiary, releaseAmount);

        emit TokenReleased(timelockId, timelock.beneficiary, releaseAmount);
    }

    /**
     * @notice Release for beneficiary
     * @param beneficiary Beneficiary address
     */
    function releaseFor(address beneficiary) external {
        bytes32[] storage timelockIds = beneficiaryTimelocks[beneficiary];
        uint256 totalRelease;

        for (uint256 i = 0; i < timelockIds.length; i++) {
            Timelock storage timelock = timelocks[timelockIds[i]];
            
            if (timelock.amount == 0 || timelock.revoked) {
                continue;
            }

            if (block.timestamp >= timelock.releaseTime) {
                require(
                    msg.sender == timelock.beneficiary || msg.sender == owner,
                    "not authorized"
                );

                uint256 releaseAmount = timelock.amount - timelock.released;
                if (releaseAmount > 0) {
                    timelock.released += releaseAmount;
                    totalRelease += releaseAmount;
                }
            }
        }

        if (totalRelease > 0) {
            totalReleased += totalRelease;
            totalLocked -= totalRelease;
            _transferToken(beneficiary, totalRelease);
            emit TokenReleased(
                keccak256(abi.encodePacked(beneficiary)),
                beneficiary,
                totalRelease
            );
        }
    }

    // =============================================================================
    // REVOCATION
    // =============================================================================

    /**
     * @notice Revoke timelock
     * @param timelockId Timelock ID
     */
    function revoke(bytes32 timelockId) external onlyOwner {
        Timelock storage timelock = timelocks[timelockId];
        
        if (timelock.amount == 0) revert Unauthorized();
        if (timelock.revoked) revert AlreadyRevoked();

        // Calculate remaining
        uint256 remaining = timelock.amount - timelock.released;
        
        // Mark as revoked
        timelock.revoked = true;

        // Update totals
        if (remaining > 0) {
            totalLocked -= remaining;
        }

        emit TimelockRevoked(timelockId);
    }

    // =============================================================================
    // QUERY FUNCTIONS
    // =============================================================================

    /**
     * @notice Get pending release amount
     * @param timelockId Timelock ID
     * @return pending Amount pending release
     */
    function getPendingAmount(bytes32 timelockId)
        external
        view
        returns (uint256 pending)
    {
        Timelock storage timelock = timelocks[timelockId];
        
        if (timelock.amount == 0 || timelock.revoked) {
            return 0;
        }

        if (block.timestamp >= timelock.releaseTime) {
            return timelock.amount - timelock.released;
        }

        return 0;
    }

    /**
     * @notice Get timelock status
     * @param timelockId Timelock ID
     * @return released Release status
     */
    function getTimelockStatus(bytes32 timelockId)
        external
        view
        returns (bool released, bool revoked, uint256 releaseAmount)
    {
        Timelock storage timelock = timelocks[timelockId];
        
        if (timelock.amount == 0) {
            return (false, false, 0);
        }

        released = block.timestamp >= timelock.releaseTime;
        revoked = timelock.revoked;
        
        if (released && !revoked) {
            releaseAmount = timelock.amount - timelock.released;
        }
    }

    /**
     * @notice Get timelocks for beneficiary
     * @param beneficiary Beneficiary address
     * @return Array of timelock IDs
     */
    function getTimelocks(address beneficiary)
        external
        view
        returns (bytes32[] memory)
    {
        return beneficiaryTimelocks[beneficiary];
    }

    /**
     * @notice Get total pending for beneficiary
     * @param beneficiary Beneficiary address
     * @return pending Total pending amount
     */
    function getPendingFor(address beneficiary)
        external
        view
        returns (uint256 pending)
    {
        bytes32[] storage timelockIds = beneficiaryTimelocks[beneficiary];

        for (uint256 i = 0; i < timelockIds.length; i++) {
            Timelock storage timelock = timelocks[timelockIds[i]];
            
            if (timelock.amount == 0 || timelock.revoked) {
                continue;
            }

            if (block.timestamp >= timelock.releaseTime) {
                pending += timelock.amount - timelock.released;
            }
        }
    }

    // =============================================================================
    // HELPER FUNCTIONS
    // =============================================================================

    /**
     * @notice Transfer tokens
     * @param to Recipient
     * @param amount Amount
     */
    function _transferToken(address to, uint256 amount) internal {
        // Simplified - in production would use IERC20
        (to, amount);
    }

    /**
     * @notice Set owner
     * @param newOwner New owner
     */
    function setOwner(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        owner = newOwner;
    }

    /**
     * @notice Set token
     * @param newToken New token
     */
    function setToken(address newToken) external onlyOwner {
        if (newToken == address(0)) revert Unauthorized();
        token = newToken;
    }

    // =============================================================================
    // INIT
    // =============================================================================

    // Avoid unused warnings
    function init() external pure {
        require(totalLocked == 0, "init check");
    }
}