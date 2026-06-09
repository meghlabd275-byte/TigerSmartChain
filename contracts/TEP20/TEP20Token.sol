// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title TEP20 Token Standard
/// @notice Token standard for TigerSmartChain

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/ERC20Pausable.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

/// @notice TEP20 Token Implementation
/// @dev Full-featured token with burning, pausing, and access control
contract TEP20 is ERC20, ERC20Burnable, ERC20Pausable, AccessControl {
    
    /// @notice Role for minters
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    
    /// @notice Role for pausers
    bytes32 public constant PAUSER_ROLE = keccak256("PAUSER_ROLE");
    
    /// @notice Token decimals
    uint8 private _decimals;
    
    /// @notice Maximum supply
    uint256 private _maxSupply;
    
    /// @notice Tax rate (in basis points)
    uint256 public taxRate;
    
    /// @notice Tax recipient
    address public taxRecipient;
    
    /// @notice Blacklist mapping
    mapping(address => bool) public blacklist;
    
    /// @notice Anti-bot protection
    bool public antiBotEnabled;
    
    /// @notice Transfer cooldown (in seconds)
    mapping(address => uint256) public transferCooldown;
    
    /// @notice Events
    event TaxRateUpdated(uint256 newRate);
    event TaxRecipientUpdated(address newRecipient);
    event BlacklistUpdated(address indexed account, bool status);
    event AntiBotUpdated(bool enabled);
    event CooldownUpdated(address indexed account, uint256 cooldown);
    
    /// @notice Constructor
    /// @param name Token name
    /// @param symbol Token symbol
    /// @param decimals Token decimals
    /// @param maxSupply Maximum supply
    constructor(
        string memory name,
        string memory symbol,
        uint8 decimals,
        uint256 maxSupply
    ) ERC20(name, symbol) {
        _decimals = decimals;
        _maxSupply = maxSupply;
        
        // Grant roles to deployer
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        _grantRole(MINTER_ROLE, msg.sender);
        _grantRole(PAUSER_ROLE, msg.sender);
    }
    
    /// @notice Override decimals
    function decimals() public view override returns (uint8) {
        return _decimals;
    }
    
    /// @notice Override max supply
    function maxSupply() public view returns (uint256) {
        return _maxSupply;
    }
    
    /// @notice Mint new tokens
    /// @param to Recipient address
    /// @param amount Amount to mint
    function mint(address to, uint256 amount) public onlyRole(MINTER_ROLE) {
        require(totalSupply() + amount <= _maxSupply, "TEP20: exceeds max supply");
        _mint(to, amount);
    }
    
    /// @notice Burn tokens
    /// @param amount Amount to burn
    function burn(uint256 amount) public override(ERC20Burnable) {
        _burn(_msgSender(), amount);
    }
    
    /// @notice Pause token transfers
    function pause() public onlyRole(PAUSER_ROLE) {
        _pause();
    }
    
    /// @notice Unpause token transfers
    function unpause() public onlyRole(PAUSER_ROLE) {
        _unpause();
    }
    
    /// @notice Set tax rate
    /// @param newRate New tax rate (in basis points)
    function setTaxRate(uint256 newRate) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(newRate <= 1000, "TEP20: tax too high");
        taxRate = newRate;
        emit TaxRateUpdated(newRate);
    }
    
    /// @notice Set tax recipient
    /// @param recipient New tax recipient
    function setTaxRecipient(address recipient) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(recipient != address(0), "TEP20: invalid recipient");
        taxRecipient = recipient;
        emit TaxRecipientUpdated(recipient);
    }
    
    /// @notice Update blacklist
    /// @param account Account to update
    /// @param status New blacklist status
    function setBlacklist(address account, bool status) external onlyRole(DEFAULT_ADMIN_ROLE) {
        blacklist[account] = status;
        emit BlacklistUpdated(account, status);
    }
    
    /// @notice Enable/disable anti-bot
    /// @param enabled Anti-bot status
    function setAntiBot(bool enabled) external onlyRole(DEFAULT_ADMIN_ROLE) {
        antiBotEnabled = enabled;
        emit AntiBotUpdated(enabled);
    }
    
    /// @notice Set transfer cooldown
    /// @param account Account to set
    /// @param cooldown Cooldown in seconds
    function setTransferCooldown(address account, uint256 cooldown) external onlyRole(DEFAULT_ADMIN_ROLE) {
        transferCooldown[account] = cooldown;
        emit CooldownUpdated(account, cooldown);
    }
    
    /// @notice Hook before transfer
    function _beforeTokenTransfer(
        address from,
        address to,
        uint256 amount
    ) internal override(ERC20, ERC20Pausable) {
        // Check blacklist
        require(!blacklist[from], "TEP20: sender blacklisted");
        require(!blacklist[to], "TEP20: recipient blacklisted");
        
        // Check anti-bot
        if (antiBotEnabled && from != address(0) && to != address(0)) {
            require(
                block.timestamp >= transferCooldown[from],
                "TEP20: cooldown active"
            );
        }
        
        super._beforeTokenTransfer(from, to, amount);
    }
    
    /// @notice Override transfer with tax
    function _update(address from, address to, uint256 amount) 
        internal override(ERC20) returns (uint256) {
        
        if (taxRate > 0 && taxRecipient != address(0) && from != address(0) && to != address(0)) {
            uint256 tax = (amount * taxRate) / 10000;
            
            if (tax > 0) {
                super._update(from, taxRecipient, tax);
                amount -= tax;
            }
        }
        
        return super._update(from, to, amount);
    }
}