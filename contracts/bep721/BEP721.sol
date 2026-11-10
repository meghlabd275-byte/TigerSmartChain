// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title BEP721 Token Standard
/// @notice NFT Standard for Binance Smart Chain

import "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import "@openzeppelin/contracts/token/ERC721/extensions/ERC721URIStorage.sol";
import "@openzeppelin/contracts/token/ERC721/extensions/ERC721Burnable.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

/// @title BEP721Token
/// @notice BEP721 NFT with metadata, burning, and access control
contract BEP721Token is ERC721, ERC721URIStorage, ERC721Burnable, AccessControl {
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes32 public constant PAUSER_ROLE = keccak256("PAUSER_ROLE");

    // Mapping from token ID to creator
    mapping(uint256 => address) private _tokenCreators;

    // Mapping from creator to token count
    mapping(address => uint256) private _creatorTokenCount;

    // Base URI
    string private _baseTokenURI;

    // Contract URI for marketplace
    string public contractURI;

    // Total supply
    uint256 private _totalSupply;

    // Token ID counter
    uint256 private _currentTokenId;

    // Addresses authorised to sign lazy-mint vouchers.
    mapping(address => bool) private _lazyMintSigners;

    // Voucher hashes already redeemed, so a signature cannot be replayed.
    mapping(bytes32 => bool) private _redeemedVouchers;

    /// @notice Emitted when a lazy-mint signer is added or removed
    event LazyMintSignerUpdated(address indexed signer, bool allowed);

    /// @notice Emitted when a lazy-minted token is created
    event LazyMinted(address indexed to, uint256 indexed tokenId, string uri);

    /// @notice Emitted when creator is queried
    event CreatorQueried(address indexed creator, uint256 tokenId);

    /// @notice Emitted when batch minted
    event BatchMinted(address indexed to, uint256[] tokenIds);

    /// @notice Contract constructor
    /// @param name Token name
    /// @param symbol Token symbol
    /// @param baseURI Base URI for tokens
    constructor(
        string memory name,
        string memory symbol,
        string memory baseURI
    ) ERC721(name, symbol) {
        _baseTokenURI = baseURI;
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        _grantRole(MINTER_ROLE, msg.sender);
        _grantRole(PAUSER_ROLE, msg.sender);
    }

    /// @notice Returns the total number of tokens
    function totalSupply() public view returns (uint256) {
        return _totalSupply;
    }

    /// @notice Returns the creator of tokenId
    function creatorOf(uint256 tokenId) public view returns (address) {
        require(_ownerOf(tokenId) != address(0), "BEP721: nonexistent token");
        return _tokenCreators[tokenId];
    }

    /// @notice Mint a new token
    /// @param to Address to mint token to
    /// @param uri Token URI
    function mint(address to, string memory uri) public returns (uint256) {
        require(hasRole(MINTER_ROLE, msg.sender), "Must have minter role");

        _currentTokenId++;
        uint256 newTokenId = _currentTokenId;

        _safeMint(to, newTokenId);
        _setTokenURI(newTokenId, uri);
        _tokenCreators[newTokenId] = msg.sender;
        _creatorTokenCount[msg.sender]++;

        return newTokenId;
    }

    /// @notice Batch mint tokens
    /// @param to Address to mint tokens to
    /// @param uris Array of token URIs
    function batchMint(address to, string[] memory uris) public returns (uint256[] memory) {
        require(hasRole(MINTER_ROLE, msg.sender), "Must have minter role");
        require(uris.length > 0, "Empty array");

        uint256[] memory tokenIds = new uint256[](uris.length);

        for (uint256 i = 0; i < uris.length; i++) {
            _currentTokenId++;
            uint256 newTokenId = _currentTokenId;

            _safeMint(to, newTokenId);
            _setTokenURI(newTokenId, uris[i]);
            _tokenCreators[newTokenId] = msg.sender;
            _creatorTokenCount[msg.sender]++;

            tokenIds[i] = newTokenId;
        }

        emit BatchMinted(to, tokenIds);

        return tokenIds;
    }

    /// @notice Authorises an address to sign lazy-mint vouchers
    function setLazyMintSigner(address signer, bool allowed)
        external
        onlyRole(DEFAULT_ADMIN_ROLE)
    {
        require(signer != address(0), "BEP721: zero signer");
        _lazyMintSigners[signer] = allowed;
        emit LazyMintSignerUpdated(signer, allowed);
    }

    /// @notice Returns whether an address may sign lazy-mint vouchers
    function isLazyMintSigner(address signer) external view returns (bool) {
        return _lazyMintSigners[signer];
    }

    /// @notice Hash that a lazy-mint voucher signature must commit to
    function lazyMintHash(address to, string memory uri, uint256 tokenId, uint256 deadline)
        public
        view
        returns (bytes32)
    {
        return keccak256(
            abi.encodePacked(
                address(this),
                block.chainid,
                to,
                tokenId,
                keccak256(bytes(uri)),
                deadline
            )
        );
    }

    /// @notice Mints a token against a voucher signed by an authorised signer
    /// @dev The caller must be the voucher's `to` address, the token id is
    ///      fixed by the signer, and each voucher can be redeemed only once.
    function lazyMint(
        string memory uri,
        uint256 tokenId,
        uint256 deadline,
        bytes memory signature
    ) external returns (uint256) {
        require(block.timestamp <= deadline, "BEP721: voucher expired");
        require(_tokenCreators[tokenId] == address(0), "BEP721: voucher redeemed");
        require(bytes(signature).length == 65, "BEP721: bad signature length");

        bytes32 digest = MessageHashUtils.toEthSignedMessageHash(
            lazyMintHash(msg.sender, uri, tokenId, deadline)
        );
        address signer = ECDSA.recover(digest, signature);
        require(_lazyMintSigners[signer], "BEP721: unauthorised signer");

        _redeemedVouchers[digest] = true;

        _safeMint(msg.sender, tokenId);
        _setTokenURI(tokenId, uri);
        _tokenCreators[tokenId] = msg.sender;
        _creatorTokenCount[msg.sender]++;

        emit LazyMinted(msg.sender, tokenId, uri);
        return tokenId;
    }

    /// @notice Returns whether a voucher digest has already been redeemed
    function voucherRedeemed(bytes32 digest) external view returns (bool) {
        return _redeemedVouchers[digest];
    }

    /// @notice Set base URI
    /// @param baseURI New base URI
    function setBaseURI(string memory baseURI) public onlyRole(DEFAULT_ADMIN_ROLE) {
        _baseTokenURI = baseURI;
    }

    /// @notice Set contract URI for marketplace
    /// @param uri Contract URI
    function setContractURI(string memory uri) public onlyRole(DEFAULT_ADMIN_ROLE) {
        contractURI = uri;
    }

    /// @notice Get token IDs by creator
    /// @param creator Creator address
    /// @return Array of token IDs
    function tokensByCreator(address creator) public view returns (uint256[] memory) {
        uint256 count = _creatorTokenCount[creator];
        uint256[] memory result = new uint256[](count);
        uint256 index = 0;

        for (uint256 i = 1; i <= _currentTokenId; i++) {
            if (_tokenCreators[i] == creator) {
                result[index] = i;
                index++;
            }
        }

        return result;
    }

    /// @notice Get tokens owned by address
    /// @param owner Owner address
    /// @return Array of owned token IDs
    function tokensOfOwner(address owner) public view returns (uint256[] memory) {
        return _tokensOfOwner(owner);
    }

    /// @dev Internal function to get tokens of owner
    function _tokensOfOwner(address owner) internal view returns (uint256[] memory) {
        uint256 balance = balanceOf(owner);
        uint256[] memory result = new uint256[](balance);
        uint256 index = 0;

        for (uint256 i = 1; i <= _currentTokenId; i++) {
            if (ownerOf(i) == owner) {
                result[index] = i;
                index++;
            }
        }

        return result;
    }

    /// @dev Override tokenURI to support ERC721A
    function tokenURI(uint256 tokenId)
        public
        view
        virtual
        override(ERC721, ERC721URIStorage)
        returns (string memory)
    {
        require(_ownerOf(tokenId) != address(0), "BEP721: nonexistent token");
        return super.tokenURI(tokenId);
    }

    /// @dev Override base URI
    function _baseURI() internal view virtual override returns (string memory) {
        return _baseTokenURI;
    }

    /// @dev Override supportsInterface
    function supportsInterface(bytes4 interfaceId)
        public
        view
        override(ERC721, ERC721URIStorage, AccessControl)
        returns (bool)
    {
        return super.supportsInterface(interfaceId);
    }

    /// @dev Tracks supply on burn. `_update` is the only virtual hook that
    ///      fires for every token movement in OpenZeppelin v5.
    function _update(
        address to,
        uint256 tokenId,
        address auth
    ) internal virtual override returns (address) {
        address from = super._update(to, tokenId, auth);

        if (from == address(0)) {
            _totalSupply++;
        } else if (to == address(0)) {
            _totalSupply--;
            address creator = _tokenCreators[tokenId];
            if (creator != address(0)) {
                _creatorTokenCount[creator]--;
            }
        }

        return from;
    }
}

/// @title BEP721Collection - Collection with owner minting
/// @notice Allow collection owners to mint
contract BEP721Collection is BEP721Token {
    mapping(address => bool) public approvedMinters;

    modifier onlyApprovedMinter() {
        require(
            approvedMinters[msg.sender] || hasRole(DEFAULT_ADMIN_ROLE, msg.sender),
            "Not approved minter"
        );
        _;
    }

    constructor(
        string memory name,
        string memory symbol,
        string memory baseURI
    ) BEP721Token(name, symbol, baseURI) {}

    /// @notice Add approved minter
    function addApprovedMinter(address minter) external onlyRole(DEFAULT_ADMIN_ROLE) {
        approvedMinters[minter] = true;
    }

    /// @notice Remove approved minter
    function removeApprovedMinter(address minter) external onlyRole(DEFAULT_ADMIN_ROLE) {
        approvedMinters[minter] = false;
    }

    /// @notice Mint by approved minter
    function mintByApprovedMinter(address to, string memory uri)
        external
        onlyApprovedMinter
        returns (uint256)
    {
        return mint(to, uri);
    }
}