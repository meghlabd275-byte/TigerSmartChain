// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title Oracle
 * @dev Price oracle for cross-chain price feeds
 * @notice Provides price data for BNB, tokens, and other assets
 */
contract Oracle {
    // =============================================================================
    // DATA STRUCTURES
    // =============================================================================

    // Price data structure
    struct PriceData {
        uint256 price;
        uint256 timestamp;
        uint256 confidence;
    }

    // Rate data for cross-chain
    struct RateData {
        uint256 srcRate;      // Source chain rate
        uint256 dstRate;     // Destination chain rate
        uint256 lastUpdate; // Last update timestamp
    }

    // =============================================================================
    // STATE VARIABLES
    // =============================================================================

    // Owner
    address public owner;

    // Price data by asset
    mapping(bytes32 => PriceData) public prices;

    // Cross-chain rates
    mapping(bytes32 => RateData) public crossChainRates;

    // Feeder addresses (authorized price feeders)
    mapping(address => bool) public feeders;

    // Chain ID to rate
    mapping(uint256 => uint256) public chainRates;

    // Emergency stop
    bool public stopped;

    // Minimum confidence threshold
    uint256 public minConfidence;

    // Stale price threshold (5 minutes)
    uint256 public staleThreshold;

    // Event for price updates
    event PriceUpdated(bytes32 indexed asset, uint256 price, uint256 timestamp);
    event PriceFeedEnabled(address indexed feeder, bool enabled);
    event EmergencyStop(bool stopped);

    // =============================================================================
    // MODIFIERS
    // =============================================================================

    modifier onlyOwner() {
        require(msg.sender == owner, "Oracle: not owner");
        _;
    }

    modifier onlyFeeder() {
        require(feeders[msg.sender] || msg.sender == owner, "Oracle: not feeder");
        _;
    }

    modifier notStopped() {
        require(!stopped, "Oracle: stopped");
        _;
    }

    // =============================================================================
    // CONSTRUCTOR
    // =============================================================================

    constructor() {
        owner = msg.sender;
        feeders[msg.sender] = true;
        minConfidence = 500000000000000000; // 50% confidence
        staleThreshold = 5 minutes;
    }

    // =============================================================================
    // PRICE MANAGEMENT
    // =============================================================================

    /**
     * @notice Update price for an asset
     * @param asset Asset identifier
     * @param price New price
     * @param confidence Confidence level (0-100%)
     */
    function updatePrice(
        bytes32 asset,
        uint256 price,
        uint256 confidence
    ) external onlyFeeder notStopped {
        require(price > 0, "Oracle: invalid price");
        require(confidence >= minConfidence, "Oracle: low confidence");

        prices[asset] = PriceData({
            price: price,
            timestamp: block.timestamp,
            confidence: confidence
        });

        emit PriceUpdated(asset, price, block.timestamp);
    }

    /**
     * @notice Get current price for an asset
     * @param asset Asset identifier
     * @return price Current price
     * @return timestamp Last update time
     */
    function getPrice(bytes32 asset) public view returns (uint256 price, uint256 timestamp) {
        PriceData memory data = prices[asset];
        require(data.price > 0, "Oracle: price not set");
        require(
            block.timestamp - data.timestamp <= staleThreshold,
            "Oracle: stale price"
        );

        return (data.price, data.timestamp);
    }

    /**
     * @notice Get price with validation
     * @param asset Asset identifier
     * @return price Price data
     */
    function getPriceValidated(bytes32 asset) public view returns (PriceData memory) {
        PriceData memory data = prices[asset];
        require(data.price > 0, "Oracle: price not set");
        require(
            block.timestamp - data.timestamp <= staleThreshold,
            "Oracle: stale price"
        );
        require(data.confidence >= minConfidence, "Oracle: low confidence");

        return data;
    }

    // =============================================================================
    // CROSS-CHAIN RATES
    // =============================================================================

    /**
     * @notice Update cross-chain rate
     * @param srcChain Source chain ID
     * @param dstChain Destination chain ID
     * @param rate Cross-chain rate
     */
    function updateCrossChainRate(
        uint256 srcChain,
        uint256 dstChain,
        uint256 rate
    ) external onlyFeeder notStopped {
        bytes32 key = keccak256(abi.encodePacked(srcChain, dstChain));

        crossChainRates[key] = RateData({
            srcRate: rate,
            dstRate: rate,
            lastUpdate: block.timestamp
        });

        chainRates[srcChain] = rate;
    }

    /**
     * @notice Get cross-chain rate
     * @param srcChain Source chain ID
     * @param dstChain Destination chain ID
     * @return rate Cross-chain rate
     */
    function getCrossChainRate(uint256 srcChain, uint256 dstChain) public view returns (uint256) {
        bytes32 key = keccak256(abi.encodePacked(srcChain, dstChain));
        RateData memory data = crossChainRates[key];

        require(data.lastUpdate > 0, "Oracle: rate not set");
        require(
            block.timestamp - data.lastUpdate <= staleThreshold,
            "Oracle: stale rate"
        );

        return data.srcRate;
    }

    // =============================================================================
    // FEEDER MANAGEMENT
    // =============================================================================

    /**
     * @notice Enable or disable a feeder
     * @param feeder Feeder address
     * @param enabled Enable status
     */
    function setFeeder(address feeder, bool enabled) external onlyOwner {
        feeders[feeder] = enabled;
        emit PriceFeedEnabled(feeder, enabled);
    }

    // =============================================================================
    // EMERGENCY CONTROLS
    // =============================================================================

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
     * @param newOwner New owner address
     */
    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "Oracle: zero address");
        owner = newOwner;
    }

    // =============================================================================
    // UTILITY FUNCTIONS
    // =============================================================================

    /**
     * @notice Calculate exchange amount
     * @param srcAmount Source amount
     * @param srcChain Source chain ID
     * @param dstChain Destination chain ID
     * @return dstAmount Destination amount
     */
    function calculateExchange(
        uint256 srcAmount,
        uint256 srcChain,
        uint256 dstChain
    ) external view returns (uint256 dstAmount) {
        uint256 rate = getCrossChainRate(srcChain, dstChain);
        dstAmount = (srcAmount * rate) / 1e18;
    }

    /**
     * @notice Get multiple prices
     * @param assets Array of asset identifiers
     * @return pricesArray Array of prices
     */
    function getMultiplePrices(bytes32[] calldata assets)
        external
        view
        returns (uint256[] memory pricesArray)
    {
        pricesArray = new uint256[](assets.length);

        for (uint256 i = 0; i < assets.length; i++) {
            (pricesArray[i], ) = getPrice(assets[i]);
        }
    }

    // =============================================================================
    // TIME CONSTANTS
    // =============================================================================

    function init() private pure {
        // Avoid unused variable warning
        uint256 x = 1 minutes;
        require(x == 60 seconds, "time check");
    }
}