// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title TEP2981 NFT Royalty Standard
/// @notice Standard for NFT royalty information
/// @dev Based on EIP-2981

/// @notice Royalty info interface
interface ITEP2981 {
    /// @notice Returns royalty amount for a given token
    /// @param tokenId Token ID
    /// @param salePrice Sale price of the token
    /// @return receiver Royalty recipient address
    /// @return royaltyAmount Royalty amount in wei
    function royaltyInfo(uint256 tokenId, uint256 salePrice)
        external
        view
        returns (address receiver, uint256 royaltyAmount);

    /// @notice Event when royalty is updated
    event RoyaltyUpdated(
        uint256 indexed tokenId,
        address indexed recipient,
        uint256 feeBasisPoint
    );
}

/// @title Royalty Collector
/// @dev Contract to manage NFT royalties
contract RoyaltyCollector is ITEP2981 {
    struct RoyaltyInfo {
        address recipient;
        uint256 feeBasisPoints; // 10000 = 100%
    }

    // Token ID to royalty info
    mapping(uint256 => RoyaltyInfo) private _royaltyInfo;

    // Collection-wide royalty (applies to all tokens if not overridden)
    address private _defaultRecipient;
    uint256 private _defaultFeeBasisPoints;

    // Maximum royalty fee (50%)
    uint256 public constant MAX_FEE_BASIS_POINTS = 5000;

    /// @notice Constructor
    /// @param defaultRecipient Default royalty recipient
    /// @param defaultFeeBasisPoints Default fee in basis points
    constructor(address defaultRecipient, uint256 defaultFeeBasisPoints) {
        require(defaultFeeBasisPoints <= MAX_FEE_BASIS_POINTS, "TEP2981: fee too high");
        _defaultRecipient = defaultRecipient;
        _defaultFeeBasisPoints = defaultFeeBasisPoints;
    }

    /// @notice See ITEP2981.royaltyInfo
    function royaltyInfo(uint256 tokenId, uint256 salePrice)
        external
        view
        override
        returns (address receiver, uint256 royaltyAmount)
    {
        RoyaltyInfo memory info = _royaltyInfo[tokenId];

        // Use token-specific or default
        if (info.recipient == address(0)) {
            receiver = _defaultRecipient;
            royaltyAmount = (salePrice * _defaultFeeBasisPoints) / 10000;
        } else {
            receiver = info.recipient;
            royaltyAmount = (salePrice * info.feeBasisPoints) / 10000;
        }
    }

    /// @notice Sets royalty for a specific token
    /// @param tokenId Token ID
    /// @param recipient Royalty recipient
    /// @param feeBasisPoints Fee in basis points
    function setTokenRoyalty(
        uint256 tokenId,
        address recipient,
        uint256 feeBasisPoints
    ) external {
        require(feeBasisPoints <= MAX_FEE_BASIS_POINTS, "TEP2981: fee too high");
        require(recipient != address(0), "TEP2981: zero address");

        _royaltyInfo[tokenId] = RoyaltyInfo({
            recipient: recipient,
            feeBasisPoints: feeBasisPoints
        });

        emit RoyaltyUpdated(tokenId, recipient, feeBasisPoints);
    }

    /// @notice Sets default royalty for collection
    /// @param recipient Royalty recipient
    /// @param feeBasisPoints Fee in basis points
    function setDefaultRoyalty(address recipient, uint256 feeBasisPoints) external {
        require(feeBasisPoints <= MAX_FEE_BASIS_POINTS, "TEP2981: fee too high");
        require(recipient != address(0), "TEP2981: zero address");

        _defaultRecipient = recipient;
        _defaultFeeBasisPoints = feeBasisPoints;
    }

    /// @notice Returns royalty info for a token
    function getRoyaltyInfo(uint256 tokenId)
        external
        view
        returns (address recipient, uint256 feeBasisPoints)
    {
        RoyaltyInfo memory info = _royaltyInfo[tokenId];
        if (info.recipient == address(0)) {
            return (_defaultRecipient, _defaultFeeBasisPoints);
        }
        return (info.recipient, info.feeBasisPoints);
    }
}

/// @title NFT with Royalty
/// @dev NFT contract with built-in royalty support
contract NFTWithRoyalty is RoyaltyCollector {
    mapping(uint256 => address) private _creators;

    /// @notice Constructor
    /// @param name Token name
    /// @param symbol Token symbol
    /// @param defaultRecipient Default royalty recipient
    /// @param defaultFeeBasisPoints Default fee
    constructor(
        string memory name,
        string memory symbol,
        address defaultRecipient,
        uint256 defaultFeeBasisPoints
    ) RoyaltyCollector(defaultRecipient, defaultFeeBasisPoints) {}

    /// @notice Mint with creator
    /// @param to Mint to address
    /// @param tokenId Token ID
    /// @param creator Creator address
    function mintWithCreator(
        address to,
        uint256 tokenId,
        address creator
    ) external {
        _creators[tokenId] = creator;
    }

    /// @notice Get creator of token
    function creatorOf(uint256 tokenId) external view returns (address) {
        return _creators[tokenId];
    }

    /// @notice Calculate royalty for a sale
    /// @param tokenId Token ID
    /// @param salePrice Sale price
    /// @return The royalty amount
    function calculateRoyalty(uint256 tokenId, uint256 salePrice)
        external
        view
        returns (uint256)
    {
        (, uint256 amount) = royaltyInfo(tokenId, salePrice);
        return amount;
    }
}