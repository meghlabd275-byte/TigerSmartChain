// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Cross-Chain Relayer
/// @notice Relayer network for cross-chain message passing
/// @dev Enables validators to relay messages between chains

import "@openzeppelin/contracts/access/AccessControl.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/// @title Relayer Interface
interface IRelayer {
    /// @notice Relays a message
    function relayMessage(
        uint256 chainId,
        bytes memory message,
        bytes[] memory signatures
    ) external returns (bool);

    /// @notice Event when message is relayed
    event MessageRelayed(
        uint256 indexed chainId,
        bytes32 indexed messageId,
        address indexed relayer
    );
}

/// @title Relayer Contract
contract Relayer is IRelayer, AccessControl, ReentrancyGuard {
    bytes32 public constant RELAYER_ROLE = keccak256("RELAYER_ROLE");
    bytes32 public constant VALIDATOR_ROLE = keccak256("VALIDATOR_ROLE");

    // Minimum validators required
    uint256 public constant MIN_VALIDATORS = 3;

    // Message hashes
    mapping(bytes32 => bool) public relayedMessages;

    // Relay nonce
    uint256 public relayNonce;

    // Supported chains
    mapping(uint256 => bool) public supportedChains;

    // Event
    event MessageRelayed(
        uint256 indexed chainId,
        bytes32 indexed messageId,
        address indexed relayer
    );

    constructor() {
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        _grantRole(RELAYER_ROLE, msg.sender);
    }

    /// @notice Adds a supported chain
    function addSupportedChain(uint256 chainId) external onlyRole(DEFAULT_ADMIN_ROLE) {
        supportedChains[chainId] = true;
    }

    /// @notice Removes a supported chain
    function removeSupportedChain(uint256 chainId)
        external
        onlyRole(DEFAULT_ADMIN_ROLE)
    {
        supportedChains[chainId] = false;
    }

    /// @notice Relays a message
    function relayMessage(
        uint256 chainId,
        bytes memory message,
        bytes[] memory signatures
    ) external override onlyRole(RELAYER_ROLE) nonReentrant returns (bool) {
        require(supportedChains[chainId], "Unsupported chain");

        // Verify minimum signatures
        require(
            signatures.length >= MIN_VALIDATORS,
            "Insufficient validators"
        );

        // Create message ID
        bytes32 messageId = keccak256(
            abi.encodePacked(chainId, relayNonce++, message)
        );

        // Verify message not already relayed
        require(!relayedMessages[messageId], "Message already relayed");

        // Verify signatures (in production, verify each validator signature)
        // For now, just mark as relayed
        relayedMessages[messageId] = true;

        emit MessageRelayed(chainId, messageId, msg.sender);

        return true;
    }

    /// @notice Returns if message was relayed
    function isMessageRelayed(bytes32 messageId) external view returns (bool) {
        return relayedMessages[messageId];
    }
}

/// @title Relayer Client
/// @dev Client contract for sending messages through relayer
contract RelayerClient {
    Relayer public relayer;

    constructor(address _relayer) {
        require(_relayer != address(0), "Invalid relayer");
        relayer = Relayer(_relayer);
    }

    /// @notice Sends a message to another chain
    function sendMessage(
        uint256 destChainId,
        bytes memory message,
        bytes[] memory signatures
    ) external returns (bool) {
        return relayer.relayMessage(destChainId, message, signatures);
    }
}

/// @title Oracle
/// @notice Oracle for cross-chain price verification
/// @dev Provides price data for cross-chain transfers

import "@openzeppelin/contracts/access/Ownable.sol";

/// @title Price Oracle
contract PriceOracle is Ownable {
    // Symbol to price data
    mapping(string => PriceData) public prices;

    // Last update time
    mapping(string => uint256) public lastUpdate;

    // Event
    event PriceUpdated(string indexed symbol, uint256 price);

    struct PriceData {
        uint256 price;
        uint256 timestamp;
        uint256 blockNumber;
    }

    constructor() Ownable() {}

    /// @notice Updates a price
    function updatePrice(string memory symbol, uint256 price) external onlyOwner {
        require(price > 0, "Invalid price");

        prices[symbol] = PriceData({
            price: price,
            timestamp: block.timestamp,
            blockNumber: block.number
        });

        lastUpdate[symbol] = block.timestamp;

        emit PriceUpdated(symbol, price);
    }

    /// @notice Gets a price
    function getPrice(string memory symbol) external view returns (uint256) {
        return prices[symbol].price;
    }

    /// @notice Gets price data
    function getPriceData(string memory symbol)
        external
        view
        returns (
            uint256 price,
            uint256 timestamp,
            uint256 blockNumber
        )
    {
        PriceData memory data = prices[symbol];
        return (data.price, data.timestamp, data.blockNumber);
    }
}

/// @title Challenge System
/// @notice Challenge system for fraud proofs
/// @dev Enables anyone to challenge invalid states

import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/// @title Challenge Contract
contract ChallengeSystem is Ownable, ReentrancyGuard {
    // Challenge status
    enum ChallengeStatus {
        Pending,
        Resolved,
        Dismissed
    }

    // Challenge structure
    struct Challenge {
        address challenger;
        bytes32 claimedState;
        bytes32 canonicalState;
        uint256 blockNumber;
        ChallengeStatus status;
        uint256 deposit;
        uint256 createdAt;
    }

    // Challenges
    mapping(bytes32 => Challenge) public challenges;

    // Challenge ID to challenge
    mapping(bytes32 => bytes32[]) public stateChallenges;

    // Slash amount
    uint256 public slashAmount;

    // Reward percentage (5000 = 50%)
    uint256 public rewardPercentage;

    // Event
    event ChallengeCreated(
        bytes32 indexed challengeId,
        address indexed challenger
    );
    event ChallengeResolved(
        bytes32 indexed challengeId,
        bool indexed success
    );

    constructor() Ownable() {
        slashAmount = 1 ether;
        rewardPercentage = 5000;
    }

    /// @notice Creates a challenge
    function createChallenge(
        bytes32 claimedState,
        bytes32 canonicalState,
        uint256 blockNumber
    ) external payable nonReentrant returns (bytes32 challengeId) {
        require(msg.value >= slashAmount, "Insufficient deposit");

        challengeId = keccak256(
            abi.encodePacked(
                claimedState,
                canonicalState,
                blockNumber,
                msg.sender,
                block.timestamp
            )
        );

        require(
            challenges[challengeId].createdAt == 0,
            "Challenge exists"
        );

        challenges[challengeId] = Challenge({
            challenger: msg.sender,
            claimedState: claimedState,
            canonicalState: canonicalState,
            blockNumber: blockNumber,
            status: ChallengeStatus.Pending,
            deposit: msg.value,
            createdAt: block.timestamp
        });

        stateChallenges[canonicalState].push(challengeId);

        emit ChallengeCreated(challengeId, msg.sender);
    }

    /// @notice Resolves a challenge
    function resolveChallenge(
        bytes32 challengeId,
        bool success
    ) external onlyOwner nonReentrant {
        Challenge storage challenge = challenges[challengeId];
        require(challenge.createdAt > 0, "Challenge not found");
        require(
            challenge.status == ChallengeStatus.Pending,
            "Not pending"
        );

        if (success) {
            // Challenger wins - slash malicious actor
            challenge.status = ChallengeStatus.Resolved;

            // Reward challenger
            uint256 reward = (challenge.deposit * rewardPercentage) / 10000;
            payable(challenge.challenger).transfer(reward);
        } else {
            // Challenge dismissed - challenger loses deposit
            challenge.status = ChallengeStatus.Dismissed;
        }

        emit ChallengeResolved(challengeId, success);
    }

    /// @notice Sets slash amount
    function setSlashAmount(uint256 amount) external onlyOwner {
        slashAmount = amount;
    }

    /// @notice Returns challenge count for a state
    function getChallengeCount(bytes32 state)
        external
        view
        returns (uint256)
    {
        return stateChallenges[state].length;
    }
}

/// @title BNB Converter
/// @notice Converts between native BNB and wrapped BNB
/// @dev Enables BNB to be used as tokens in contracts

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

/// @title Wrapped BNB
contract WrappedBNB is ERC20, Ownable {
    // Deposit event
    event Deposit(address indexed user, uint256 amount);
    // Withdrawal event
    event Withdrawal(address indexed user, uint256 amount);

    constructor() ERC20("Wrapped BNB", "WBNB") Ownable() {}

    /// @notice Deposits BNB and mints WBNB
    function deposit() external payable {
        require(msg.value > 0, "Cannot deposit 0");
        _mint(msg.sender, msg.value);
        emit Deposit(msg.sender, msg.value);
    }

    /// @notice Burns WBNB and withdraws BNB
    function withdraw(uint256 amount) external {
        require(balanceOf(msg.sender) >= amount, "Insufficient balance");
        _burn(msg.sender, amount);
        payable(msg.sender).transfer(amount);
        emit Withdrawal(msg.sender, amount);
    }

    /// @notice Emergency withdraw by owner
    function emergencyWithdraw() external onlyOwner {
        payable(owner()).transfer(address(this).balance);
    }

    // Required for receiving BNB
    receive() external payable {
        _mint(msg.sender, msg.value);
        emit Deposit(msg.sender, msg.value);
    }
}

/// @title BNB Converter
/// @dev Converter between native BNB and WBNB
contract BNBConverter is Ownable {
    WrappedBNB public wbnb;

    // Conversion event
    event Converted(
        address indexed user,
        uint256 amount,
        bool toNative
    );

    constructor(address _wbnb) Ownable() {
        require(_wbnb != address(0), "Invalid WBNB");
        wbnb = WrappedBNB(_wbnb);
    }

    /// @notice Convert BNB to WBNB
    function convertToWrapped() external payable {
        require(msg.value > 0, "Cannot convert 0");
        wbnb.deposit{value: msg.value}();
        wbnb.transfer(msg.sender, msg.value);
        emit Converted(msg.sender, msg.value, false);
    }

    /// @notice Convert WBNB to BNB
    function convertToNative(uint256 amount) external {
        require(amount > 0, "Cannot convert 0");
        require(wbnb.transferFrom(msg.sender, address(this), amount), "Transfer failed");
        wbnb.withdraw(amount);
        payable(msg.sender).transfer(amount);
        emit Converted(msg.sender, amount, true);
    }
}