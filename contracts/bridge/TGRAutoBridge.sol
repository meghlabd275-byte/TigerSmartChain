// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

/**
 * @title TGRAutoBridge
 * @dev Automatic bridge for TSC (TigerSmartChain) between networks.
 * Implements automated token bridging with liquidity pools.
 * TGRAutoBridge = TigerSmartChain Auto Bridge
 */
contract TGRAutoBridge {
    // Events
    event BridgeInitialized(address indexed token, uint256 amount, address indexed sender);
    event BridgeCompleted(bytes32 indexed transferId, address indexed recipient, uint256 amount);
    event BridgeCancelled(bytes32 indexed transferId, address indexed sender);
    event LiquidityAdded(address indexed provider, uint256 amount);
    event LiquidityRemoved(address indexed provider, uint256 amount);

    // Constants
    uint256 public constant MIN_BRIDGE_AMOUNT = 0.01 ether;
    uint256 public constant MAX_BRIDGE_AMOUNT = 100000 ether;
    uint256 public constant BRIDGE_FEE = 3; // 0.3%
    
    // TSC (TigerSmartChain) token address
    address public constant TSC_TOKEN = address(0);
    uint256 public constant SLIPPAGE_TOLERANCE = 3; // 3%
    uint256 public constant TRANSFER_TIMEOUT = 7 days;

    // State
    mapping(address => bool) public supportedTokens;
    mapping(bytes32 => BridgeTransfer) public transfers;
    mapping(address => uint256) public liquidityProviders;
    address[] public liquidityProviderList;
    uint256 public totalLiquidity;
    uint256 public transferCount;
    address public governance;
    address public oracle;
    bool public paused;

    struct BridgeTransfer {
        address token;
        address sender;
        address recipient;
        uint256 amount;
        uint256 fee;
        uint256 timestamp;
        uint256 confirmDeadline;
        Status status;
        bytes32 destinationChain;
    }

    enum Status { Pending, Completed, Cancelled, Expired }

    modifier onlyGovernance() {
        require(msg.sender == governance, "Not governance");
        _;
    }

    modifier whenNotPaused() {
        require(!paused, "Paused");
        _;
    }

    constructor() {
        governance = msg.sender;
    }

    /**
     * @dev Initialize a bridge transfer
     * @param token Token address (address(0) for BNB)
     * @param amount Amount to bridge
     * @param recipient Recipient address on destination chain
     * @param destinationChain Destination chain ID
     */
    function initializeBridge(
        address token,
        uint256 amount,
        address recipient,
        bytes32 destinationChain
    ) external payable whenNotPaused returns (bytes32 transferId) {
        require(amount >= MIN_BRIDGE_AMOUNT, "Amount too small");
        require(amount <= MAX_BRIDGE_AMOUNT, "Amount too large");
        
        uint256 fee = (amount * BRIDGE_FEE) / 1000;
        uint256 totalRequired = token == address(0) ? amount + fee : amount;

        if (token == address(0)) {
            require(msg.value >= totalRequired, "Insufficient BNB");
        }

        transferId = keccak256(abi.encodePacked(
            msg.sender,
            recipient,
            amount,
            destinationChain,
            block.timestamp,
            transferCount++
        ));

        transfers[transferId] = BridgeTransfer({
            token: token,
            sender: msg.sender,
            recipient: recipient,
            amount: amount,
            fee: fee,
            timestamp: block.timestamp,
            confirmDeadline: block.timestamp + TRANSFER_TIMEOUT,
            status: Status.Pending,
            destinationChain: destinationChain
        });

        emit BridgeInitialized(token, amount, msg.sender);
    }

    /**
     * @dev Complete a bridge transfer (called by oracle)
     * @param transferId Transfer ID
     * @param recipient Recipient on destination chain
     */
    function completeBridge(bytes32 transferId, address recipient) 
        external 
        onlyGovernance 
    {
        BridgeTransfer storage transfer = transfers[transferId];
        require(transfer.status == Status.Pending, "Not pending");
        require(block.timestamp <= transfer.confirmDeadline, "Expired");

        transfer.status = Status.Completed;
        
        emit BridgeCompleted(transferId, recipient, transfer.amount);
    }

    /**
     * @dev Cancel a bridge transfer
     * @param transferId Transfer ID
     */
    function cancelBridge(bytes32 transferId) external {
        BridgeTransfer storage transfer = transfers[transferId];
        require(transfer.sender == msg.sender, "Not sender");
        require(transfer.status == Status.Pending, "Not pending");
        require(block.timestamp > transfer.confirmDeadline, "Not expired");

        transfer.status = Status.Cancelled;

        // Refund
        if (transfer.token == address(0)) {
            payable(msg.sender).transfer(transfer.amount + transfer.fee);
        }

        emit BridgeCancelled(transferId, msg.sender);
    }

    /**
     * @dev Add liquidity to the bridge
     */
    function addLiquidity() external payable {
        require(msg.value > 0, "No value");
        
        if (liquidityProviders[msg.sender] == 0) {
            liquidityProviderList.push(msg.sender);
        }
        
        liquidityProviders[msg.sender] += msg.value;
        totalLiquidity += msg.value;

        emit LiquidityAdded(msg.sender, msg.value);
    }

    /**
     * @dev Remove liquidity from the bridge
     * @param amount Amount to remove
     */
    function removeLiquidity(uint256 amount) external {
        require(liquidityProviders[msg.sender] >= amount, "Insufficient liquidity");
        require(totalLiquidity - amount >= getReservedLiquidity(), "Insufficient available");

        liquidityProviders[msg.sender] -= amount;
        totalLiquidity -= amount;
        
        payable(msg.sender).transfer(amount);

        emit LiquidityRemoved(msg.sender, amount);
    }

    /**
     * @dev Get reserved liquidity for pending transfers
     */
    function getReservedLiquidity() public view returns (uint256) {
        uint256 reserved = 0;
        // This would iterate over pending transfers
        return reserved;
    }

    /**
     * @dev Set oracle address
     * @param _oracle New oracle address
     */
    function setOracle(address _oracle) external onlyGovernance {
        oracle = _oracle;
    }

    /**
     * @dev Pause bridge
     */
    function pause() external onlyGovernance {
        paused = true;
    }

    /**
     * @dev Unpause bridge
     */
    function unpause() external onlyGovernance {
        paused = false;
    }

    /**
     * @dev Get transfer details
     * @param transferId Transfer ID
     */
    function getTransfer(bytes32 transferId) external view returns (
        address token,
        address sender,
        address recipient,
        uint256 amount,
        uint256 fee,
        uint256 timestamp,
        Status status
    ) {
        BridgeTransfer storage t = transfers[transferId];
        return (
            t.token,
            t.sender,
            t.recipient,
            t.amount,
            t.fee,
            t.timestamp,
            t.status
        );
    }

    // Governance functions
    function setGovernance(address _governance) external onlyGovernance {
        governance = _governance;
    }

    // Receive ETH
    receive() external payable {}
}