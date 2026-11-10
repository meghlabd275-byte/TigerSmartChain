// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

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
        public
        view
        virtual
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

        _setTokenRoyalty(tokenId, recipient, feeBasisPoints);
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

    /// @notice Sets royalty for a token, for use by derived contracts.
    function _setTokenRoyalty(
        uint256 tokenId,
        address recipient,
        uint256 feeBasisPoints
    ) internal {
        _royaltyInfo[tokenId] = RoyaltyInfo({
            recipient: recipient,
            feeBasisPoints: feeBasisPoints
        });
        emit RoyaltyUpdated(tokenId, recipient, feeBasisPoints);
    }

    /// @notice Returns royalty info for a token
    function getRoyaltyInfo(uint256 tokenId)
        public
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
contract NFTWithRoyalty is ERC721, RoyaltyCollector, Ownable {
    mapping(uint256 => address) private _creators;

    // Metadata URI per token; ERC721 keeps its own private map that cannot be
    // written to from a derived contract.
    mapping(uint256 => string) private _tokenURIs;

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
    ) ERC721(name, symbol) RoyaltyCollector(defaultRecipient, defaultFeeBasisPoints) Ownable(msg.sender) {}

    /// @notice Mint with creator
    /// @param to Mint to address
    /// @param tokenId Token ID
    /// @param creator Creator address
    function mintWithCreator(
        address to,
        uint256 tokenId,
        address creator
    ) external onlyOwner {
        _mint(to, tokenId);
        _creators[tokenId] = creator;

        // A minted token earns the collection default until the creator or
        // owner overrides it.
        (, uint256 defaultFee) = getRoyaltyInfo(tokenId);
        if (defaultFee != 0) {
            _setTokenRoyalty(tokenId, creator, defaultFee);
        }
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

    /// @inheritdoc ERC721
    function tokenURI(uint256 tokenId)
        public
        view
        override
        returns (string memory)
    {
        _requireOwned(tokenId);
        return _tokenURIs[tokenId];
    }

    /// @notice Sets the metadata URI for a token.
    function setTokenURI(uint256 tokenId, string memory uri) external {
        require(
            _ownerOf(tokenId) == msg.sender || getApproved(tokenId) == msg.sender,
            "NFTWithRoyalty: not authorised"
        );
        _tokenURIs[tokenId] = uri;
        emit MetadataUpdate(tokenId);
    }

    event MetadataUpdate(uint256 indexed tokenId);
}
