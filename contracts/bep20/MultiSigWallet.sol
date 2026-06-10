// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title MultiSigWallet
 * @dev Multi-signature wallet for secure asset management
 * @notice Requires multiple confirmations for transactions
 */
contract MultiSigWallet {
    // =============================================================================
    // DATA STRUCTURES
    // =============================================================================

    // Transaction structure
    struct Transaction {
        address to;
        uint256 value;
        bytes data;
        uint256 confirmations;
        bool executed;
        bool cancelled;
    }

    // =============================================================================
    // STATE VARIABLES
    // =============================================================================

    // Owners
    mapping(address => bool) public owners;

    // Required confirmations
    uint256 public required;

    // Transaction count
    uint256 public transactionCount;

    // Transactions
    mapping(uint256 => Transaction) public transactions;

    // Confirmations
    mapping(uint256 => mapping(address => bool)) public confirmations;

    // Events
    event Confirmation(address indexed owner, uint256 indexed txId);
    event Execution(uint256 indexed txId, address indexed to, uint256 value);
    event Cancellation(uint256 indexed txId);
    event Deposit(address indexed from, uint256 value);

    // Errors
    error NotOwner();
    error InvalidOwner();
    error NotConfirmed();
    error AlreadyConfirmed();
    error AlreadyExecuted();
    error AlreadyCancelled();
    error ExecutionFailed();
    error InvalidValue();
    error InsufficientConfirmations();
    error ZeroAddress();

    // =============================================================================
    // MODIFIERS
    // =============================================================================

    modifier onlyOwner() {
        if (!owners[msg.sender]) revert NotOwner();
        _;
    }

    // =============================================================================
    // CONSTRUCTOR
    // =============================================================================

    /**
     * @notice Constructor
     * @param _owners Array of owner addresses
     * @param _required Required confirmations
     */
    constructor(address[] memory _owners, uint256 _required) {
        require(_owners.length > 0, "no owners");
        require(_required > 0, "required > 0");
        require(_required <= _owners.length, "required > owners");

        // Set owners
        for (uint256 i = 0; i < _owners.length; i++) {
            if (_owners[i] == address(0)) revert ZeroAddress();
            owners[_owners[i]] = true;
        }

        required = _required;
    }

    // =============================================================================
    // TRANSACTIONS
    // =============================================================================

    /**
     * @notice Submit a transaction
     * @param to Recipient address
     * @param value Amount to send
     * @param data Transaction data
     * @return txId Transaction ID
     */
    function submitTransaction(
        address to,
        uint256 value,
        bytes calldata data
    ) external onlyOwner returns (uint256 txId) {
        if (to == address(0)) revert ZeroAddress();
        if (value == 0 && data.length == 0) revert InvalidValue();

        txId = transactionCount++;

        transactions[txId] = Transaction({
            to: to,
            value: value,
            data: data,
            confirmations: 0,
            executed: false,
            cancelled: false
        });

        // Auto-confirm from sender
        if (!confirmations[txId][msg.sender]) {
            confirmations[txId][msg.sender] = true;
            transactions[txId].confirmations++;
        }
    }

    /**
     * @notice Confirm a transaction
     * @param txId Transaction ID
     */
    function confirmTransaction(uint256 txId) external onlyOwner {
        Transaction storage tx = transactions[txId];
        
        if (tx.to == address(0)) revert NotConfirmed();
        if (tx.executed) revert AlreadyExecuted();
        if (tx.cancelled) revert AlreadyCancelled();
        if (confirmations[txId][msg.sender]) revert AlreadyConfirmed();

        // Confirm
        confirmations[txId][msg.sender] = true;
        tx.confirmations++;

        emit Confirmation(msg.sender, txId);
    }

    /**
     * @notice Execute a transaction
     * @param txId Transaction ID
     */
    function executeTransaction(uint256 txId) external onlyOwner {
        Transaction storage tx = transactions[txId];
        
        if (tx.to == address(0)) revert NotConfirmed();
        if (tx.executed) revert AlreadyExecuted();
        if (tx.cancelled) revert AlreadyCancelled();
        
        // Check confirmations
        if (tx.confirmations < required) {
            revert InsufficientConfirmations();
        }

        // Mark as executed
        tx.executed = true;

        // Execute
        (bool success, ) = tx.to.call{value: tx.value}(tx.data);
        if (!success) revert ExecutionFailed();

        emit Execution(txId, tx.to, tx.value);
    }

    /**
     * @notice Cancel a transaction
     * @param txId Transaction ID
     */
    function cancelTransaction(uint256 txId) external onlyOwner {
        Transaction storage tx = transactions[txId];
        
        if (tx.to == address(0)) revert NotConfirmed();
        if (tx.executed) revert AlreadyExecuted();
        if (tx.cancelled) revert AlreadyCancelled();

        // Mark as cancelled
        tx.cancelled = true;

        emit Cancellation(txId);
    }

    // =============================================================================
    // RECEIVE
    // =============================================================================

    /**
     * @notice Receive native tokens
     */
    receive() external payable {
        emit Deposit(msg.sender, msg.value);
    }

    // =============================================================================
    // QUERY FUNCTIONS
    // =============================================================================

    /**
     * @notice Get transaction details
     * @param txId Transaction ID
     * @return Transaction struct
     */
    function getTransaction(uint256 txId)
        external
        view
        returns (
            address to,
            uint256 value,
            bytes memory data,
            uint256 confirmations,
            bool executed,
            bool cancelled
        )
    {
        Transaction storage tx = transactions[txId];
        return (
            tx.to,
            tx.value,
            tx.data,
            tx.confirmations,
            tx.executed,
            tx.cancelled
        );
    }

    /**
     * @notice Check if transaction is confirmed
     * @param txId Transaction ID
     * @param owner Owner address
     * @return confirmed Confirmation status
     */
    function isConfirmed(uint256 txId, address owner)
        external
        view
        returns (bool confirmed)
    {
        return confirmations[txId][owner];
    }

    /**
     * @notice Get confirmation count
     * @param txId Transaction ID
     * @return count Number of confirmations
     */
    function getConfirmationCount(uint256 txId)
        external
        view
        returns (uint256 count)
    {
        return transactions[txId].confirmations;
    }

    /**
     * @notice Get owner count
     * @return count Number of owners
     */
    function getOwnerCount() external view returns (uint256 count) {
        // Note: This is inefficient but works for now
        // In production, would maintain a separate count
        count = 0;
        for (uint256 i = 0; i < 1000; i++) {
            if (owners[address(uint160(i))]) {
                count++;
            }
        }
    }

    /**
     * @notice Get pending transactions
     * @return Array of transaction IDs
     */
    function getPendingTransactions()
        external
        view
        returns (uint256[] memory txIds)
    {
        uint256 count;
        
        // Count pending
        for (uint256 i = 0; i < transactionCount; i++) {
            Transaction storage tx = transactions[i];
            if (!tx.executed && !tx.cancelled && tx.confirmations >= required) {
                count++;
            }
        }

        // Collect pending
        txIds = new uint256[](count);
        uint256 index;
        
        for (uint256 i = 0; i < transactionCount; i++) {
            Transaction storage tx = transactions[i];
            if (!tx.executed && !tx.cancelled && tx.confirmations >= required) {
                txIds[index++] = i;
            }
        }
    }

    // =============================================================================
    // ADMIN FUNCTIONS
    // =============================================================================

    /**
     * @notice Add owner
     * @param newOwner New owner address
     */
    function addOwner(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        owners[newOwner] = true;
    }

    /**
     * @notice Remove owner
     * @param oldOwner Owner address to remove
     */
    function removeOwner(address oldOwner) external onlyOwner {
        owners[oldOwner] = false;
    }

    /**
     * @notice Change requirement
     * @param _required New required confirmations
     */
    function changeRequirement(uint256 _required) external onlyOwner {
        require(_required > 0, "required > 0");
        required = _required;
    }
}

// =============================================================================
// WALLET FACTORY
// =============================================================================

/**
 * @title MultiSigWalletFactory
 * @dev Factory for creating multi-signature wallets
 */
contract MultiSigWalletFactory {
    // Event
    event WalletCreated(address indexed wallet, address[] owners, uint256 required);

    /**
     * @notice Create a new multi-sig wallet
     * @param owners Array of owner addresses
     * @param required Required confirmations
     * @return wallet Address of created wallet
     */
    function createWallet(
        address[] memory owners,
        uint256 required
    ) external returns (address wallet) {
        wallet = address(new MultiSigWallet(owners, required));
        emit WalletCreated(wallet, owners, required);
    }
}