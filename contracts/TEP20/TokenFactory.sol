// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Token Factory
/// @notice Factory for creating TEP20 tokens
/// @dev Enables permissionless token creation with预设参数

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

/// @notice Token Factory Interface
interface ITokenFactory {
    /// @notice Creates a new token
    /// @param name Token name
    /// @param symbol Token symbol
    /// @param decimals Token decimals
    /// @param initialSupply Initial supply
    /// @param cap Token cap (0 for uncapped)
    /// @param owner Token owner (address(0) for no owner)
    /// @return tokenAddress Created token address
    function createToken(
        string memory name,
        string memory symbol,
        uint8 decimals,
        uint256 initialSupply,
        uint256 cap,
        address owner
    ) external returns (address tokenAddress);

    /// @notice Event when token is created
    event TokenCreated(
        address indexed tokenAddress,
        string name,
        string symbol,
        uint8 decimals,
        uint256 initialSupply,
        address indexed creator
    );
}

/// @title Basic Token
/// @dev Basic TEP20 implementation for factory tokens
contract BasicToken is ERC20 {
    uint8 private _decimals;

    constructor(
        string memory name,
        string memory symbol,
        uint8 decimals_,
        uint256 initialSupply,
        address initialHolder
    ) ERC20(name, symbol) {
        _decimals = decimals_;
        _mint(initialHolder, initialSupply);
    }

    function decimals() public view override returns (uint8) {
        return _decimals;
    }
}

/// @title Token Factory Implementation
contract TokenFactory is ITokenFactory, Ownable {
    // Mapping from token address to creator
    mapping(address => address) public tokenCreators;

    // Counter for deterministic addresses
    uint256 public tokenCount;

    // Fee for creating token (in BNB)
    uint256 public creationFee;

    // Fee recipient
    address public feeRecipient;

    // Allowed token implementations
    mapping(address => bool) public allowedImplementations;

    constructor() Ownable() {
        creationFee = 0.01 ether;
        feeRecipient = msg.sender;
    }

    /// @notice Creates a new token
    function createToken(
        string memory name,
        string memory symbol,
        uint8 decimals,
        uint256 initialSupply,
        uint256 cap,
        address owner
    ) external payable override returns (address tokenAddress) {
        // Check fee
        require(msg.value >= creationFee, "Insufficient creation fee");

        // Send fee to recipient
        (bool success, ) = feeRecipient.call{value: msg.value}("");
        require(success, "Fee transfer failed");

        // Create token
        BasicToken token = new BasicToken(
            name,
            symbol,
            decimals,
            initialSupply,
            owner == address(0) ? msg.sender : owner
        );

        tokenAddress = address(token);
        tokenCreators[tokenAddress] = msg.sender;

        emit TokenCreated(
            tokenAddress,
            name,
            symbol,
            decimals,
            initialSupply,
            msg.sender
        );
    }

    /// @notice Sets creation fee
    function setCreationFee(uint256 fee) external onlyOwner {
        creationFee = fee;
    }

    /// @notice Sets fee recipient
    function setFeeRecipient(address recipient) external onlyOwner {
        require(recipient != address(0), "Invalid recipient");
        feeRecipient = recipient;
    }

    /// @notice Returns token creator
    function getTokenCreator(address token) external view returns (address) {
        return tokenCreators[token];
    }

    /// @notice Returns all tokens created by an address
    function getTokensByCreator(address creator)
        external
        view
        returns (address[] memory)
    {
        // In production, use indexed mapping
        return new address[](0);
    }
}

/// @title Token Factory with Presets
/// @dev Factory with preset configurations
contract TokenFactoryWithPresets is TokenFactory {
    enum Preset {
        Standard,    // 18 decimals, no cap
        Stablecoin,  // 6 decimals, no cap
        NFT,         // 0 decimals, capped
        Utility     // 9 decimals, capped
    }

    // Preset configurations
    mapping(Preset => bytes) public presets;

    constructor() TokenFactory() {
        // Set presets
    }

    /// @notice Creates token with preset
    function createWithPreset(
        string memory name,
        string memory symbol,
        Preset preset,
        uint256 initialSupply,
        address owner
    ) external payable returns (address tokenAddress) {
        uint8 decimals;
        uint256 cap;

        if (preset == Preset.Standard) {
            decimals = 18;
            cap = 0;
        } else if (preset == Preset.Stablecoin) {
            decimals = 6;
            cap = 0;
        } else if (preset == Preset.NFT) {
            decimals = 0;
            cap = initialSupply;
        } else {
            decimals = 9;
            cap = initialSupply * 1000;
        }

        return createToken(
            name,
            symbol,
            decimals,
            initialSupply,
            cap,
            owner
        );
    }
}