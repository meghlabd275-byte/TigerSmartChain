// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Cross-Chain Token Bridge
/// @notice Bridge for transferring tokens across chains

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/// @title TokenHub
/// @notice Token Hub for cross-chain transfers
contract TokenHub is AccessControl, ReentrancyGuard {
    using SafeERC20 for IERC20;

    /// @notice Relayer role
    bytes32 public constant RELAYER_ROLE = keccak256("RELAYER_ROLE");

    /// @notice Chain ID
    uint16 public chainId;

    /// @notice Maximum transfer amount
    uint256 public maxTransferAmount;

    /// @notice Minimum transfer amount
    uint256 public minTransferAmount;

    /// @notice Relay fee
    uint256 public relayFee;

    /// @notice Supported tokens
    mapping(address => bool) public supportedTokens;

    /// @notice Nonces for cross-chain transfers
    mapping(address => uint64) public nonces;

    /// @notice Transfer records
    mapping(bytes32 => bool) public transferRecords;

    /// @notice Contract addresses on other chains
    mapping(uint16 => mapping(address => address)) public peers;

    /// @notice Events
    event TokenSent(
        address indexed token,
        address indexed sender,
        address indexed recipient,
        uint256 amount,
        uint16 toChain,
        uint64 nonce,
        bytes32 transferId
    );

    event TokenReceived(
        address indexed token,
        address indexed recipient,
        uint256 amount,
        uint16 fromChain,
        uint64 nonce,
        bytes32 transferId
    );

    event RelayFeeUpdated(uint256 newFee);
    event MaxTransferAmountUpdated(uint256 newAmount);
    event MinTransferAmountUpdated(uint256 newAmount);
    event TokenSupportUpdated(address indexed token, bool supported);
    event PeerSet(uint16 indexed chainId, address indexed token, address indexed peer);

    /// @notice Constructor
    /// @param _chainId Chain ID
    constructor(uint16 _chainId) {
        chainId = _chainId;
        maxTransferAmount = type(uint256).max;
        minTransferAmount = 1;
        relayFee = 0;
        
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
    }

    /// @notice Send tokens to another chain
    /// @param token Token address
    /// @param toChain Target chain ID
    /// @param recipient Recipient address on target chain
    /// @param amount Amount to send
    function sendToken(
        address token,
        uint16 toChain,
        address recipient,
        uint256 amount
    ) external payable nonReentrant returns (bytes32) {
        require(supportedTokens[token], "Token not supported");
        require(amount >= minTransferAmount, "Amount too low");
        require(amount <= maxTransferAmount, "Amount too high");
        require(peers[toChain][token] != address(0), "Peer not set");
        require(recipient != address(0), "Invalid recipient");

        // Check fee
        if (relayFee > 0) {
            require(msg.value >= relayFee, "Insufficient fee");
        }

        // Get token
        IERC20(token).safeTransferFrom(msg.sender, address(this), amount);

        // Generate transfer ID
        uint64 nonce = nonces[msg.sender]++;
        bytes32 transferId = generateTransferId(
            token,
            msg.sender,
            recipient,
            amount,
            toChain,
            nonce
        );

        // Record transfer
        transferRecords[transferId] = true;

        emit TokenSent(
            token,
            msg.sender,
            recipient,
            amount,
            toChain,
            nonce,
            transferId
        );

        return transferId;
    }

    /// @notice Relay received tokens (called by relayer)
    /// @param token Token address
    /// @param sender Sender on source chain
    /// @param recipient Recipient
    /// @param amount Amount
    /// @param fromChain Source chain ID
    /// @param nonce Nonce
    /// @param transferId Transfer ID
    /// @param signature Relayer signature
    function relayToken(
        address token,
        address sender,
        address recipient,
        uint256 amount,
        uint16 fromChain,
        uint64 nonce,
        bytes32 transferId,
        bytes calldata signature
    ) external onlyRole(RELAYER_ROLE) nonReentrant returns (bool) {
        require(supportedTokens[token], "Token not supported");
        require(!transferRecords[transferId], "Already relayed");
        require(recipient != address(0), "Invalid recipient");

        // Verify signature
        bytes32 message = keccak256(
            abi.encodePacked(
                token,
                sender,
                recipient,
                amount,
                fromChain,
                nonce,
                transferId
            )
        );
        
        // Verify relayer signature
        // In production, use proper signature verification
        
        // Mark as relayed
        transferRecords[transferId] = true;

        // Transfer tokens
        IERC20(token).safeTransfer(recipient, amount);

        emit TokenReceived(
            token,
            recipient,
            amount,
            fromChain,
            nonce,
            transferId
        );

        return true;
    }

    /// @notice Generate transfer ID
    function generateTransferId(
        address token,
        address sender,
        address recipient,
        uint256 amount,
        uint16 toChain,
        uint64 nonce
    ) public pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                token,
                sender,
                recipient,
                amount,
                toChain,
                nonce,
                block.chainid
            )
        );
    }

    /// @notice Set peer contract on another chain
    function setPeer(uint16 _chainId, address token, address peer) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(peer != address(0), "Invalid peer");
        peers[_chainId][token] = peer;
        emit PeerSet(_chainId, token, peer);
    }

    /// @notice Enable/disable token support
    function setTokenSupport(address token, bool supported) external onlyRole(DEFAULT_ADMIN_ROLE) {
        supportedTokens[token] = supported;
        emit TokenSupportUpdated(token, supported);
    }

    /// @notice Set relay fee
    function setRelayFee(uint256 fee) external onlyRole(DEFAULT_ADMIN_ROLE) {
        relayFee = fee;
        emit RelayFeeUpdated(fee);
    }

    /// @notice Set max transfer amount
    function setMaxTransferAmount(uint256 amount) external onlyRole(DEFAULT_ADMIN_ROLE) {
        maxTransferAmount = amount;
        emit MaxTransferAmountUpdated(amount);
    }

    /// @notice Set min transfer amount
    function setMinTransferAmount(uint256 amount) external onlyRole(DEFAULT_ADMIN_ROLE) {
        minTransferAmount = amount;
        emit MinTransferAmountUpdated(amount);
    }

    /// @notice Add relayer
    function addRelayer(address relayer) external onlyRole(DEFAULT_ADMIN_ROLE) {
        grantRole(RELAYER_ROLE, relayer);
    }

    /// @notice Remove relayer
    function removeRelayer(address relayer) external onlyRole(DEFAULT_ADMIN_ROLE) {
        revokeRole(RELAYER_ROLE, relayer);
    }

    /// @notice Get nonce for address
    function getNonce(address account) external view returns (uint64) {
        return nonces[account];
    }

    /// @notice Get peer address
    function getPeer(uint16 _chainId, address token) external view returns (address) {
        return peers[_chainId][token];
    }

    /// @notice Receive ETH
    receive() external payable {}
}

/// @title Light Client
/// @notice Verifies cross-chain messages
contract LightClient is AccessControl {
    /// @notice Block headers
    mapping(uint256 => bytes32) public blockHeaders;
    
    /// @notice Current validator set
    mapping(uint256 => bytes32) public validatorSets;
    
    /// @notice Header height
    uint256 public headerHeight;
    
    /// @notice Required validators
    uint256 public requiredValidators;
    
    /// @notice Events
    event HeaderSynced(uint256 height, bytes32 hash);
    event ValidatorSetUpdated(uint256 indexedEpoch, bytes32 validatorHash);

    constructor() {
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
    }

    /// @notice Sync block header
    function syncHeader(bytes calldata header, bytes[] calldata sigs) external onlyRole(DEFAULT_ADMIN_ROLE) {
        // Verify header
        // In production, implement proper verification
        
        headerHeight++;
        blockHeaders[headerHeight] = keccak256(header);
        
        emit HeaderSynced(headerHeight, blockHeaders[headerHeight]);
    }

    /// @notice Update validator set
    function updateValidatorSet(uint256 epoch, bytes32 validatorHash) external onlyRole(DEFAULT_ADMIN_ROLE) {
        validatorSets[epoch] = validatorHash;
        
        emit ValidatorSetUpdated(epoch, validatorHash);
    }

    /// @notice Verify message
    function verifyMessage(
        uint256 blockNumber,
        bytes32 messageHash,
        bytes[] calldata signatures
    ) external view returns (bool) {
        bytes32 headerHash = blockHeaders[blockNumber];
        require(headerHash != bytes32(0), "Header not found");
        
        // Verify signatures
        // In production, implement proper threshold signature verification
        
        return true;
    }
}

/// @title Relay
/// @notice Relayer contract for cross-chain communication
contract Relay is AccessControl {
    TokenHub public tokenHub;
    LightClient public lightClient;
    
    /// @notice Relayer config
    mapping(address => bool) public approvedRelayers;
    mapping(bytes32 => bool) public processedTransfers;
    
    /// @notice Events
    event RelayExecuted(bytes32 indexed transferId, bool success);
    
    constructor(address _tokenHub, address _lightClient) {
        tokenHub = TokenHub(_tokenHub);
        lightClient = LightClient(_lightClient);
        
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
    }

    /// @notice Execute relay
    function executeRelay(
        address token,
        address sender,
        address recipient,
        uint256 amount,
        uint16 fromChain,
        uint64 nonce,
        bytes32 transferId,
        bytes calldata proof
    ) external onlyRole(DEFAULT_ADMIN_ROLE) returns (bool) {
        require(!processedTransfers[transferId], "Already processed");
        
        // Verify proof using light client
        // In production, implement proper proof verification
        
        // Mark as processed
        processedTransfers[transferId] = true;
        
        // Relay token
        bool success = tokenHub.relayToken(
            token,
            sender,
            recipient,
            amount,
            fromChain,
            nonce,
            transferId,
            proof
        );
        
        emit RelayExecuted(transferId, success);
        
        return success;
    }
}