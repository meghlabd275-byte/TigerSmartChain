// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title BEP20 Token Standard
/// @notice Token standard for Binance Smart Chain (BSC)
/// @dev Compatible with EIP-20

interface IBEP20 {
    /// @notice Returns the amount of tokens in existence.
    function totalSupply() external view returns (uint256);

    /// @notice Returns the amount of tokens owned by `account`.
    function balanceOf(address account) external view returns (uint256);

    /// @notice Moves `amount` tokens from the caller's account to `to`.
    function transfer(address to, uint256 amount) external returns (bool);

    /// @notice Returns the remaining number of tokens that `spender` will be
    function allowance(address owner, address spender) external view returns (uint256);

    /// @notice Sets `amount` as the allowance of `spender` over the caller's tokens.
    function approve(address spender, uint256 amount) external returns (bool);

    /// @notice Moves `amount` tokens from `from` to `to` using the
    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    /// @notice Emitted when `value` tokens are moved from one account (`from`) to
    /// another (`to`).
    event Transfer(address indexed from, address indexed to, uint256 value);

    /// @notice Emitted when the allowance of a `spender` for an `owner` is set by
    /// a call to {approve}. `value` is the new allowance.
    event Approval(address indexed owner, address indexed spender, uint256 value);
}

/// @title BEP20 Token Standard with Additional Features
abstract contract BEP20 is IBEP20 {
    using SafeMath for uint256;

    mapping(address => uint256) private _balances;
    mapping(address => mapping(address => uint256)) private _allowances;
    uint256 private _totalSupply;

    string private _name;
    string private _symbol;
    uint8 private _decimals;

    /// @notice Emitted when tokens are burned
    event Burn(address indexed from, uint256 value);

    /// @notice Emitted when tokens are minted
    event Mint(address indexed to, uint256 value);

    /// @dev Emitted when paused
    event Paused(address account);

    /// @dev Emitted when unpaused
    event Unpaused(address account);

    /// @notice Returns the name of the token.
    function name() public view virtual returns (string memory) {
        return _name;
    }

    /// @notice Returns the symbol of the token.
    function symbol() public view virtual returns (string memory) {
        return _symbol;
    }

    /// @notice Returns the decimals of the token.
    function decimals() public view virtual returns (uint8) {
        return _decimals;
    }

    /// @notice See {IBEP20-totalSupply}.
    function totalSupply() public view virtual override returns (uint256) {
        return _totalSupply;
    }

    /// @notice See {IBEP20-balanceOf}.
    function balanceOf(address account) public view virtual override returns (uint256) {
        return _balances[account];
    }

    /// @notice See {IBEP20-transfer}.
    function transfer(address to, uint256 amount) public virtual override returns (bool) {
        _transfer(_msgSender(), to, amount);
        return true;
    }

    /// @notice See {IBEP20-allowance}.
    function allowance(address owner, address spender) public view virtual override returns (uint256) {
        return _allowances[owner][spender];
    }

    /// @notice See {IBEP20-approve}.
    function approve(address spender, uint256 amount) public virtual override returns (bool) {
        _approve(_msgSender(), spender, amount);
        return true;
    }

    /// @notice See {IBEP20-transferFrom}.
    function transferFrom(address from, address to, uint256 amount) public virtual override returns (bool) {
        _spendAllowance(from, _msgSender(), amount);
        _transfer(from, to, amount);
        return true;
    }

    /// @notice Atomically increases the allowance granted to `spender` by the caller.
    function increaseAllowance(address spender, uint256 addedValue) public virtual returns (bool) {
        _approve(_msgSender(), spender, _allowances[_msgSender()][spender].add(addedValue));
        return true;
    }

    /// @notice Atomically decreases the allowance granted to `spender` by the caller.
    function decreaseAllowance(address spender, uint256 subtractedValue) public virtual returns (bool) {
        uint256 currentAllowance = _allowances[_msgSender()][spender];
        require(currentAllowance >= subtractedValue, "BEP20: decreased allowance below zero");
        _approve(_msgSender(), spender, currentAllowance.sub(subtractedValue));
        return true;
    }

    /// @notice Creates `amount` tokens and assigns them to `account`, increasing the total supply.
    function _mint(address account, uint256 amount) internal virtual {
        require(account != address(0), "BEP20: mint to the zero address");
        _totalSupply = _totalSupply.add(amount);
        _balances[account] = _balances[account].add(amount);
        emit Mint(account, amount);
        emit Transfer(address(0), account, amount);
    }

    /// @notice Destroys `amount` tokens from `account`, reducing the total supply.
    function _burn(address account, uint256 amount) internal virtual {
        require(account != address(0), "BEP20: burn from the zero address");
        uint256 accountBalance = _balances[account];
        require(accountBalance >= amount, "BEP20: burn amount exceeds balance");
        _balances[account] = accountBalance.sub(amount);
        _totalSupply = _totalSupply.sub(amount);
        emit Burn(account, amount);
        emit Transfer(account, address(0), amount);
    }

    /// @dev Sets `amount` as the allowance of `spender` over the `owner` s tokens.
    function _approve(address owner, address spender, uint256 amount) internal virtual {
        require(owner != address(0), "BEP20: approve from the zero address");
        require(spender != address(0), "BEP20: approve to the zero address");
        _allowances[owner][spender] = amount;
        emit Approval(owner, spender, amount);
    }

    /// @dev Updates `owner` s allowance for `spender` based on spent `amount`.
    function _spendAllowance(address owner, address spender, uint256 amount) internal virtual {
        uint256 currentAllowance = _allowances[owner][spender];
        if (currentAllowance != type(uint256).max) {
            require(currentAllowance >= amount, "BEP20: insufficient allowance");
            _approve(owner, spender, currentAllowance.sub(amount));
        }
    }

    /// @dev Moves `amount` of tokens from `from` to `to`.
    function _transfer(address from, address to, uint256 amount) internal virtual {
        require(from != address(0), "BEP20: transfer from the zero address");
        require(to != address(0), "BEP20: transfer to the zero address");
        _beforeTokenTransfer(from, to, amount);
        uint256 fromBalance = _balances[from];
        require(fromBalance >= amount, "BEP20: transfer amount exceeds balance");
        _balances[from] = fromBalance.sub(amount);
        _balances[to] = _balances[to].add(amount);
        emit Transfer(from, to, amount);
        _afterTokenTransfer(from, to, amount);
    }

    /// @dev Hook that is called before any transfer of tokens.
    function _beforeTokenTransfer(address from, address to, uint256 amount) internal virtual {}

    /// @dev Hook that is called after any transfer of tokens.
    function _afterTokenTransfer(address from, address to, uint256 amount) internal virtual {}

    /// @dev Returns the sender of the current call.
    function _msgSender() internal view virtual returns (address) {
        return msg.sender;
    }

    /// @dev Returns the message data of the current call.
    function _msgData() internal view virtual returns (bytes calldata) {
        return msg.data;
    }
}

/// @title SafeMath
/// @dev Math operations with safety checks that revert on error.
library SafeMath {
    function tryAdd(uint256 a, uint256 b) internal pure returns (bool, uint256) {
        unchecked {
            uint256 c = a + b;
            if (c < a) return (false, 0);
            return (true, c);
        }
    }

    function trySub(uint256 a, uint256 b) internal pure returns (bool, uint256) {
        unchecked {
            if (b > a) return (false, 0);
            return (true, a - b);
        }
    }

    function tryMul(uint256 a, uint256 b) internal pure returns (bool, uint256) {
        unchecked {
            if (a == 0) return (true, 0);
            uint256 c = a * b;
            if (c / a != b) return (false, 0);
            return (true, c);
        }
    }

    function tryDiv(uint256 a, uint256 b) internal pure returns (bool, uint256) {
        unchecked {
            if (b == 0) return (false, 0);
            return (true, a / b);
        }
    }

    function tryMod(uint256 a, uint256 b) internal pure returns (bool, uint256) {
        unchecked {
            if (b == 0) return (false, 0);
            return (true, a % b);
        }
    }

    function add(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }

    function sub(uint256 a, uint256 b) internal pure returns (uint256) {
        return a - b;
    }

    function mul(uint256 a, uint256 b) internal pure returns (uint256) {
        return a * b;
    }

    function div(uint256 a, uint256 b) internal pure returns (uint256) {
        return a / b;
    }

    function mod(uint256 a, uint256 b) internal pure returns (uint256) {
        return a % b;
    }

    function sub(uint256 a, uint256 b, string memory errorMessage) internal pure returns (uint256) {
        unchecked {
            require(b <= a, errorMessage);
            return a - b;
        }
    }

    function div(uint256 a, uint256 b, string memory errorMessage) internal pure returns (uint256) {
        unchecked {
            require(b > 0, errorMessage);
            return a / b;
        }
    }

    function mod(uint256 a, uint256 b, string memory errorMessage) internal pure returns (uint256) {
        unchecked {
            require(b > 0, errorMessage);
            return a % b;
        }
    }
}

/// @title BEP20Token
/// @notice Full-featured BEP20 token with additional features
contract BEP20Token is BEP20 {
    mapping(address => bool) private _isExcludedFromFee;
    mapping(address => bool) private _blacklist;
    
    address public immutable treasury;
    address public operator;
    
    uint256 public taxRate = 200; // 2%
    uint256 public burnRate = 50; // 50% of tax = 1%
    bool public taxEnabled = true;
    bool public blacklistEnabled = true;
    
    modifier onlyOperator() {
        require(msg.sender == operator, "Not operator");
        _;
    }

    constructor(
        string memory name_,
        string memory symbol_,
        uint8 decimals_,
        uint256 initialSupply_,
        address treasury_
    ) {
        _name = name_;
        _symbol = symbol_;
        _decimals = decimals_;
        treasury = treasury_;
        operator = msg.sender;
        
        _mint(msg.sender, initialSupply_);
        _isExcludedFromFee[msg.sender] = true;
        _isExcludedFromFee[treasury] = true;
    }

    /// @notice Exclude account from fees
    function excludeFromFee(address account) external onlyOperator {
        _isExcludedFromFee[account] = true;
    }

    /// @notice Include account in fees
    function includeInFee(address account) external onlyOperator {
        _isExcludedFromFee[account] = false;
    }

    /// @notice Set tax rate
    function setTaxRate(uint256 rate) external onlyOperator {
        require(rate <= 1000, "Rate too high");
        taxRate = rate;
    }

    /// @notice Set burn rate
    function setBurnRate(uint256 rate) external onlyOperator {
        require(rate <= 100, "Rate too high");
        burnRate = rate;
    }

    /// @notice Enable/disable tax
    function setTaxEnabled(bool enabled) external onlyOperator {
        taxEnabled = enabled;
    }

    /// @notice Add to blacklist
    function addToBlacklist(address account) external onlyOperator {
        _blacklist[account] = true;
    }

    /// @notice Remove from blacklist
    function removeFromBlacklist(address account) external onlyOperator {
        _blacklist[account] = false;
    }

    /// @notice Check if blacklisted
    function isBlacklisted(address account) external view returns (bool) {
        return _blacklist[account];
    }

    /// @dev Override transfer to include tax and blacklist
    function _transfer(address from, address to, uint256 amount) internal virtual override {
        require(!blacklistEnabled || !_blacklist[from], "Blacklisted");
        require(!blacklistEnabled || !_blacklist[to], "Blacklisted");

        if (taxEnabled && !_isExcludedFromFee[from] && !_isExcludedFromFee[to]) {
            uint256 tax = amount.mul(taxRate).div(10000);
            uint256 burnAmount = tax.mul(burnRate).div(100);
            uint256 transferAmount = amount.sub(tax);

            super._transfer(from, treasury, tax.sub(burnAmount));
            super._transfer(from, address(0), burnAmount);
            super._transfer(from, to, transferAmount);
        } else {
            super._transfer(from, to, amount);
        }
    }

    /// @notice Mint tokens
    function mint(address to, uint256 amount) external onlyOperator {
        _mint(to, amount);
    }

    /// @notice Burn tokens
    function burn(uint256 amount) external {
        _burn(msg.sender, amount);
    }

    /// @notice Transfer operator role
    function transferOperator(address newOperator) external onlyOperator {
        operator = newOperator;
    }
}