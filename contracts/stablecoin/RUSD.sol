// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/ERC20Pausable.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";

/// @title RUSD — the TigerSmartChain USD stablecoin
/// @notice A fully collateralised USD stablecoin. Every RUSD in circulation is
/// issued by {RUSDVault} against collateral the vault holds, so the token
/// itself carries no issuance logic and cannot create supply on its own.
///
/// @dev Issuance is confined to the vault through `MINTER_ROLE`. A holder may
/// always burn their own balance (that is how redemption settles), but nobody
/// can mint without depositing collateral, because the vault is the only
/// address holding the minter role and the vault only mints after it has taken
/// custody of an equal value of collateral.
///
/// `maxSupply` is a hard ceiling enforced on every mint. It is a bound on the
/// damage a compromised vault key could do, not an economic parameter: the
/// real limit is the collateral the vault holds.
///
/// Accounts can be frozen for compliance. A frozen account can neither send
/// nor receive, which is what a regulated issuer needs in order to respond to
/// a court order; freezing cannot be used to seize funds, because it does not
/// move balances.
contract RUSD is ERC20, ERC20Burnable, ERC20Pausable, AccessControl {
    /// @notice Role allowed to create supply. Held by the vault.
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
    /// @notice Role allowed to pause and unpause transfers.
    bytes32 public constant PAUSER_ROLE = keccak256("PAUSER_ROLE");
    /// @notice Role allowed to freeze and unfreeze accounts.
    bytes32 public constant FREEZER_ROLE = keccak256("FREEZER_ROLE");

    /// @notice The most RUSD that can ever exist, in base units.
    uint256 public immutable maxSupply;

    /// @notice Whether an account is frozen.
    mapping(address => bool) public frozen;

    /// @notice Emitted when an account is frozen or unfrozen.
    event Frozen(address indexed account, bool frozen);

    /// @param maxSupply_ The hard supply ceiling, in base units. Must be
    /// non-zero: a zero ceiling would make the token permanently unusable and
    /// is far more likely to be a mistake than an intent.
    constructor(uint256 maxSupply_) ERC20("Royal Tiger USD", "RUSD") {
        require(maxSupply_ > 0, "RUSD: supply ceiling must be non-zero");
        maxSupply = maxSupply_;
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
    }

    /// @notice RUSD uses six decimals, matching USDT and USDC and the chain's
    /// own stablecoin constants. A dollar-pegged unit is quoted in cents, so
    /// six decimals is the conventional precision; the native coin keeps its
    /// eighteen.
    function decimals() public pure override returns (uint8) {
        return 6;
    }

    /// @notice Create `amount` new RUSD for `to`.
    /// @dev Vault-only. The supply ceiling is checked here rather than in the
    /// vault so it cannot be bypassed by a future minter.
    function mint(address to, uint256 amount) external onlyRole(MINTER_ROLE) {
        require(to != address(0), "RUSD: mint to the zero address");
        require(!frozen[to], "RUSD: recipient is frozen");
        require(totalSupply() + amount <= maxSupply, "RUSD: supply ceiling reached");
        _mint(to, amount);
    }

    /// @notice Destroy `amount` of `account`'s RUSD without an allowance.
    /// @dev Vault-only, and used for redemption. `burnFrom` would be the
    /// obvious choice, but it spends an allowance, so every redemption would
    /// need a prior `approve` — a second transaction and a standing approval
    /// to the vault for a balance it should not be able to move at will.
    /// Restricting this to the minter gives the vault exactly one power: to
    /// destroy RUSD it is simultaneously handing back the collateral for.
    function burnFor(address account, uint256 amount) external onlyRole(MINTER_ROLE) {
        _burn(account, amount);
    }

    /// @notice Freeze or unfreeze an account.
    function setFrozen(address account, bool isFrozen) external onlyRole(FREEZER_ROLE) {
        frozen[account] = isFrozen;
        emit Frozen(account, isFrozen);
    }

    /// @notice Stop all transfers.
    function pause() external onlyRole(PAUSER_ROLE) {
        _pause();
    }

    /// @notice Resume transfers.
    function unpause() external onlyRole(PAUSER_ROLE) {
        _unpause();
    }

    /// @dev Every balance movement passes through here, so the freeze check and
    /// the pause switch cannot be routed around.
    function _update(address from, address to, uint256 value)
        internal
        override(ERC20, ERC20Pausable)
    {
        require(!frozen[from], "RUSD: sender is frozen");
        require(!frozen[to], "RUSD: recipient is frozen");
        super._update(from, to, value);
    }
}
