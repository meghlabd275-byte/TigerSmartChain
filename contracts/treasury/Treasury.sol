// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Treasury Contract
/// @notice Multi-sig treasury for protocol funds

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/// @title Treasury
/// @dev Multi-sig treasury for managing protocol funds
contract Treasury is AccessControl, ReentrancyGuard {
    using SafeERC20 for IERC20;

    /// @notice Maximum number of signers
    uint256 public constant MAX_SIGNERS = 25;

    /// @notice Required confirmations
    uint256 public requiredConfirmations;

    /// @notice Signers list
    address[] public signers;

    /// @notice Signer mapping
    mapping(address => bool) public isSigner;

    /// @notice Nonce for tx
    uint256 public nonce;

    /// @notice Transaction data
    struct Transaction {
        address to;
        uint256 value;
        bytes data;
        bool executed;
    }

    /// @notice Transactions
    mapping(uint256 => Transaction) public transactions;

    /// @notice Confirmations (tx => signer => confirmed)
    mapping(uint256 => mapping(address => bool)) public confirmations;

    /// @notice Required confirmations count
    mapping(uint256 => uint256) public confirmationCount;

    /// @notice Events
    event SubmitTransaction(
        address indexed owner,
        uint256 indexed txIndex,
        address indexed to,
        uint256 value
    );

    event ConfirmTransaction(
        address indexed owner,
        uint256 indexed txIndex
    );

    event RevokeConfirmation(
        address indexed owner,
        uint256 indexed txIndex
    );

    event ExecuteTransaction(
        address indexed owner,
        uint256 indexed txIndex
    );

    /// @notice Constructor
    /// @param _signers Array of signers
    /// @param _requiredConfirmations Required confirmations
    constructor(address[] memory _signers, uint256 _requiredConfirmations) {
        require(
            _signers.length >= 3 && _signers.length <= MAX_SIGNERS,
            "Invalid signers count"
        );
        require(
            _requiredConfirmations > _signers.length / 2 &&
                _requiredConfirmations <= _signers.length,
            "Invalid confirmations"
        );

        for (uint256 i = 0; i < _signers.length; i++) {
            require(_signers[i] != address(0), "Invalid signer");
            require(!isSigner[_signers[i]], "Duplicate signer");

            isSigner[_signers[i]] = true;
            signers.push(_signers[i]);
        }

        requiredConfirmations = _requiredConfirmations;
        _grantRole(DEFAULT_ADMIN_ROLE, address(this));
    }

    /// @notice Submit transaction
    function submitTransaction(
        address to,
        uint256 value,
        bytes calldata data
    ) external nonReentrant returns (uint256) {
        require(isSigner[msg.sender], "Not a signer");

        uint256 txIndex = nonce;
        transactions[txIndex] = Transaction({
            to: to,
            value: value,
            data: data,
            executed: false
        });

        // Auto-confirm by sender
        confirmations[txIndex][msg.sender] = true;
        confirmationCount[txIndex] = 1;

        nonce++;

        emit SubmitTransaction(msg.sender, txIndex, to, value);

        return txIndex;
    }

    /// @notice Confirm transaction
    function confirmTransaction(uint256 txIndex) external {
        require(isSigner[msg.sender], "Not a signer");
        require(!confirmations[txIndex][msg.sender], "Already confirmed");
        require(!transactions[txIndex].executed, "Already executed");

        confirmations[txIndex][msg.sender] = true;
        confirmationCount[txIndex]++;

        emit ConfirmTransaction(msg.sender, txIndex);
    }

    /// @notice Revoke confirmation
    function revokeConfirmation(uint256 txIndex) external {
        require(isSigner[msg.sender], "Not a signer");
        require(confirmations[txIndex][msg.sender], "Not confirmed");
        require(!transactions[txIndex].executed, "Already executed");

        confirmations[txIndex][msg.sender] = false;
        confirmationCount[txIndex]--;

        emit RevokeConfirmation(msg.sender, txIndex);
    }

    /// @notice Execute transaction
    function executeTransaction(uint256 txIndex)
        external
        nonReentrant
        returns (bool)
    {
        require(!transactions[txIndex].executed, "Already executed");
        require(
            confirmationCount[txIndex] >= requiredConfirmations,
            "Not enough confirmations"
        );

        Transaction storage transaction = transactions[txIndex];
        transaction.executed = true;

        (bool success, ) = transaction.to.call{value: transaction.value}(
            transaction.data
        );
        require(success, "Execution failed");

        emit ExecuteTransaction(msg.sender, txIndex);

        return true;
    }

    /// @notice Get transaction count
    function getTransactionCount() external view returns (uint256) {
        return nonce;
    }

    /// @notice Get signers count
    function getSignersCount() external view returns (uint256) {
        return signers.length;
    }

    /// @notice Get signers
    function getSigners() external view returns (address[] memory) {
        return signers;
    }

    /// @notice Get confirmation count
    function getConfirmationCount(uint256 txIndex)
        external
        view
        returns (uint256)
    {
        return confirmationCount[txIndex];
    }

    /// @notice Get confirmations
    function getConfirmations(uint256 txIndex)
        external
        view
        returns (address[] memory)
    {
        address[] memory _confirmations = new address[](signers.length);
        uint256 count = 0;

        for (uint256 i = 0; i < signers.length; i++) {
            if (confirmations[txIndex][signers[i]]) {
                _confirmations[count] = signers[i];
                count++;
            }
        }

        // Resize array
        address[] memory result = new address[](count);
        for (uint256 i = 0; i < count; i++) {
            result[i] = _confirmations[i];
        }

        return result;
    }

    /// @notice Receive ETH
    receive() external payable {}
}

/// @title TreasuryVester
/// @dev Token vesting for team/advisors
contract TreasuryVester is AccessControl {
    using SafeERC20 for IERC20;

    /// @notice Recipient
    address public recipient;

    /// @notice Token
    IERC20 public token;

    /// @notice Total vesting amount
    uint256 public totalVesting;

    /// @notice Released amount
    uint256 public released;

    /// @notice Start time
    uint256 public startTime;

    /// @notice Cliff duration
    uint256 public cliffDuration;

    /// @notice Vesting duration
    uint256 public vestingDuration;

    /// @notice Revocable
    bool public revocable;

    /// @notice Revoked
    bool public revoked;

    /// @notice Vesting schedule
    struct VestingSchedule {
        uint256 totalAmount;
        uint256 startTime;
        uint256 cliffEnd;
        uint256 endTime;
        uint256 released;
    }

    VestingSchedule public schedule;

    /// @notice Events
    event TokensReleased(uint256 amount);
    event TokenVestingRevoked();

    /// @notice Constructor
    /// @param _token Token address
    /// @param _recipient Recipient address
    /// @param _totalVesting Total vesting amount
    /// @param _cliffDuration Cliff duration
    /// @param _vestingDuration Vesting duration
    /// @param _revocable Revocable
    constructor(
        address _token,
        address _recipient,
        uint256 _totalVesting,
        uint256 _cliffDuration,
        uint256 _vestingDuration,
        bool _revocable
    ) {
        require(_token != address(0), "Invalid token");
        require(_recipient != address(0), "Invalid recipient");

        token = IERC20(_token);
        recipient = _recipient;
        totalVesting = _totalVesting;
        cliffDuration = _cliffDuration;
        vestingDuration = _vestingDuration;
        revocable = _revocable;

        startTime = block.timestamp;
        schedule = VestingSchedule({
            totalAmount: _totalVesting,
            startTime: block.timestamp,
            cliffEnd: block.timestamp + _cliffDuration,
            endTime: block.timestamp + _vestingDuration,
            released: 0
        });

        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
    }

    /// @notice Release vested tokens
    function release() external {
        require(msg.sender == recipient, "Not recipient");

        uint256 vested = computeReleasableAmount();
        require(vested > 0, "No tokens to release");

        released += vested;
        token.safeTransfer(recipient, vested);

        emit TokensReleased(vested);
    }

    /// @notice Compute releasable amount
    function computeReleasableAmount() public view returns (uint256) {
        if (block.timestamp < schedule.cliffEnd) {
            return 0;
        }

        if (block.timestamp >= schedule.endTime) {
            return totalVesting - released;
        }

        uint256 timeFromStart = block.timestamp - schedule.startTime;
        uint256 vestedAmount = (totalVesting * timeFromStart) / vestingDuration;

        return vestedAmount - released;
    }

    /// @notice Revoke vesting
    function revoke() external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(revocable, "Not revocable");
        require(!revoked, "Already revoked");

        uint256 vested = computeReleasableAmount();

        revoked = true;
        totalVesting = released + vested;

        uint256 balance = token.balanceOf(address(this));
        token.safeTransfer(msg.sender, balance - vested);

        emit TokenVestingRevoked();
    }
}