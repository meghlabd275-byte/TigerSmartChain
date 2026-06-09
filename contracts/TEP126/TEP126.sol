// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title TEP126 Token V2 Standard
/// @notice Advanced token standard with enhanced features for TigerSmartChain
/// @dev Extends BEP20 with permit, minting, burning, and hooks

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/ERC20Permit.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/Pausable.sol";

/// @notice Token V2 interface with advanced features
interface ITEP126 is IERC20 {
    /// @notice Returns the token version
    function version() external view returns (string memory);

    /// @notice Returns the token decimals
    function decimals() external view returns (uint8);

    /// @notice Mints tokens to a recipient
    function mint(address to, uint256 amount) external;

    /// @notice Burns tokens from the sender
    function burn(uint256 amount) external;

    /// @notice Burns tokens from an address (requires approval)
    function burnFrom(address from, uint256 amount) external;

    /// @notice Returns the cap on total supply
    function cap() external view returns (uint256);

    /// @notice Returns if minting is enabled
    function mintingEnabled() external view returns (bool);

    /// @notice Returns the minter role admin
    function MINTER_ROLE() external view returns (bytes32);

    /// @notice Event emitted when tokens are minted
    event Minted(address indexed to, uint256 amount);

    /// @notice Event emitted when tokens are burned
    event Burned(address indexed from, uint256 amount);

    /// @notice Event emitted when cap is updated
    event CapUpdated(uint256 newCap);

    /// @notice Event emitted when minting is enabled/disabled
    event MintingEnabled(bool enabled);
}

/// @title TEP126 Token V2
/// @dev Advanced token with minting, burning, permit, and pausable features
contract TEP126 is ERC20, ERC20Burnable, ERC20Permit, AccessControl, Pausable, ITEP126 {
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    bytes32 public constant PAUSER_ROLE = keccak256("PAUSER_ROLE");

    string private _name;
    string private _symbol;
    uint8 private _decimals;

    uint256 private _cap;
    bool private _mintingEnabled;

    // Additional metadata
    string private _version;
    string private _tokenURI;

    // Chain info
    uint256 public chainId;
    bytes32 public domainSeparator;

    /// @notice Constructor for TEP126 token
    /// @param name_ Token name
    /// @param symbol_ Token symbol
    /// @param decimals_ Token decimals
    /// @param cap_ Maximum supply cap
    /// @param initialSupply Initial tokens to mint
    /// @param version_ Token version string
    constructor(
        string memory name_,
        string memory symbol_,
        uint8 decimals_,
        uint256 cap_,
        uint256 initialSupply,
        string memory version_
    ) ERC20(name_, symbol_) ERC20Permit(name_) {
        require(cap_ > 0, "TEP126: cap is 0");
        require(initialSupply <= cap_, "TEP126: initial supply exceeds cap");

        _name = name_;
        _symbol = symbol_;
        _decimals = decimals_;
        _cap = cap_;
        _version = version_;
        _mintingEnabled = true;

        // Set chain ID
        uint256 chainId;
        assembly {
            chainId := chainchainid
        }
        chainId = block.chainid;
        domainSeparator = keccak256(
            abi.encode(
                keccak256(
                    "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
                ),
                keccak256(bytes(name_)),
                keccak256(bytes(version_)),
                chainId,
                address(this)
            )
        );

        // Grant roles
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        _grantRole(MINTER_ROLE, msg.sender);
        _grantRole(PAUSER_ROLE, msg.sender);

        // Mint initial supply
        if (initialSupply > 0) {
            _mint(msg.sender, initialSupply);
        }
    }

    /// @notice Returns the token version
    function version() external view override returns (string memory) {
        return _version;
    }

    /// @notice Returns the token decimals
    function decimals() public view override returns (uint8) {
        return _decimals;
    }

    /// @notice Returns the token name
    function name() public view override(ERC20, IERC20) returns (string memory) {
        return _name;
    }

    /// @notice Returns the token symbol
    function symbol() public view override(ERC20, IERC20) returns (string memory) {
        return _symbol;
    }

    /// @notice Returns the cap on total supply
    function cap() external view override returns (uint256) {
        return _cap;
    }

    /// @notice Returns if minting is enabled
    function mintingEnabled() external view override returns (bool) {
        return _mintingEnabled;
    }

    /// @notice Returns the token URI
    function tokenURI() external view returns (string memory) {
        return _tokenURI;
    }

    /// @notice Sets the token URI
    /// @param uri New token URI
    function setTokenURI(string memory uri) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _tokenURI = uri;
    }

    /// @notice Mints tokens to a recipient
    /// @param to Recipient address
    /// @param amount Amount to mint
    function mint(address to, uint256 amount)
        public
        override
        onlyRole(MINTER_ROLE)
        whenNotPaused
    {
        require(_mintingEnabled, "TEP126: minting disabled");
        require(to != address(0), "TEP126: mint to zero address");
        require(amount > 0, "TEP126: mint zero amount");

        // Check cap
        uint256 newSupply = totalSupply() + amount;
        require(newSupply <= _cap, "TEP126: cap exceeded");

        _mint(to, amount);
        emit Minted(to, amount);
    }

    /// @notice Burns tokens from the sender
    function burn(uint256 amount) public override(ERC20Burnable, ITEP126) whenNotPaused {
        super.burn(amount);
        emit Burned(msg.sender, amount);
    }

    /// @notice Burns tokens from an address
    /// @param from Address to burn from
    /// @param amount Amount to burn
    function burnFrom(address from, uint256 amount)
        public
        override(ERC20Burnable, ITEP126)
        whenNotPaused
    {
        super.burnFrom(from, amount);
        emit Burned(from, amount);
    }

    /// @notice Transfers tokens with permit (gasless)
    /// @param from Source address
    /// @param to Destination address
    /// @param amount Amount to transfer
    /// @param deadline Deadline for permit
    /// @param v Signature v
    /// @param r Signature r
    /// @param s Signature s
    function permitTransfer(
        address from,
        address to,
        uint256 amount,
        uint256 deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external {
        // Use permit
        _permit(_owner, spender, value, deadline, v, r, s);
        _spendAllowance(_owner, spender, value);
        _transfer(_owner, to, value);
    }

    /// @notice Pauses all token transfers
    function pause() external onlyRole(PAUSER_ROLE) {
        _pause();
    }

    /// @notice Unpauses all token transfers
    function unpause() external onlyRole(PAUSER_ROLE) {
        _unpause();
    }

    /// @notice Updates the cap
    /// @param newCap New cap
    function updateCap(uint256 newCap) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(newCap > 0, "TEP126: cap is 0");
        require(newCap >= totalSupply(), "TEP126: new cap less than supply");
        _cap = newCap;
        emit CapUpdated(newCap);
    }

    /// @notice Enables/disables minting
    /// @param enabled True to enable, false to disable
    function setMintingEnabled(bool enabled) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _mintingEnabled = enabled;
        emit MintingEnabled(enabled);
    }

    /// @notice Returns the domain separator for permit
    function DOMAIN_SEPARATOR() external view returns (bytes32) {
        return domainSeparator;
    }

    /// @notice Returns the nonces for permit
    function nonces(address owner) external view returns (uint256) {
        return super.nonces(owner);
    }

    /// @notice Returns if the contract supports the given interface
    function supportsInterface(bytes4 interfaceId)
        public
        view
        override(AccessControl, IERC165)
        returns (bool)
    {
        return
            interfaceId == type(ITEP126).interfaceId ||
            super.supportsInterface(interfaceId);
    }

    // The following functions are overrides required by Solidity.

    function _update(
        address from,
        address to,
        uint256 value
    ) internal override(ERC20, Pausable) whenNotPaused {
        super._update(from, to, value);
    }
}