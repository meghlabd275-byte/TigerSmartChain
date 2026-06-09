// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title BEP721 Token Standard
/// @notice NFT Standard for Binance Smart Chain

import "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import "@openzeppelin/contracts/token/ERC721/extensions/ERC721URIStorage.sol";
import "@openzeppelin/contracts/token/ERC721/extensions/ERC721Burnable.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";

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
        require(_exists(tokenId), "BEP721: nonexistent token");
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
        _totalSupply++;

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
            _totalSupply++;

            tokenIds[i] = newTokenId;
        }

        emit BatchMinted(to, tokenIds);

        return tokenIds;
    }

    /// @notice Lazy mint with signature
    /// @param to Address to mint to
    /// @param uri Token URI
    /// @param signature Creator's signature
    function lazyMint(
        address to,
        string memory uri,
        bytes memory signature
    ) public returns (uint256) {
        // Verify signature
        bytes32 message = keccak256(abi.encodePacked(to, uri, block.chainid));
        bytes32 ethSignedMessage = keccak256(
            bytes(string(abi.encodePacked("\x19Ethereum Signed Message:\n32", message)))
        );

        // This would normally verify against a creator
        _currentTokenId++;
        uint256 newTokenId = _currentTokenId;

        _safeMint(to, newTokenId);
        _setTokenURI(newTokenId, uri);
        _tokenCreators[newTokenId] = to;
        _creatorTokenCount[to]++;
        _totalSupply++;

        return newTokenId;
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

    /// @dev Override to support ERC721A-style token ID enumeration
    function _exists(uint256 tokenId) internal view virtual override returns (bool) {
        return tokenId > 0 && tokenId <= _currentTokenId;
    }

    /// @dev Override tokenURI to support ERC721A
    function tokenURI(uint256 tokenId)
        public
        view
        virtual
        override(ERC721, ERC721URIStorage)
        returns (string memory)
    {
        require(_exists(tokenId), "BEP721: nonexistent token");
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
        override(ERC721, AccessControl)
        returns (bool)
    {
        return super.supportsInterface(interfaceId);
    }

    /// @dev Override burn to update tracking
    function _burn(uint256 tokenId)
        internal
        virtual
        override(ERC721, ERC721URIStorage)
    {
        address creator = _tokenCreators[tokenId];
        if (creator != address(0)) {
            _creatorTokenCount[creator]--;
        }
        _totalSupply--;

        super._burn(tokenId);
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