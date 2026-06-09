// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/**
 * @dev BEP-1155: TigerSmartChain Multi Token Standard
 * 
 * Implementation of the BEP-1155 standard as defined in the TigerSmartChain documentation.
 * BEP-1155 is a standard for contracts that manage multiple token types.
 * A single deployed contract may include any combination of fungible tokens, 
 * non-fungible tokens, or other configurations.
 * 
 * https://github.com/tigersmartchain/BEPs/blob/main/BEP-1155.md
 */
abstract contract Context {
    function _msgSender() internal view virtual returns (address) {
        return msg.sender;
    }

    function _msgData() internal view virtual returns (bytes calldata) {
        return msg.data;
    }
}

abstract contract ERC165 {
    function supportsInterface(bytes4 interfaceId) public view virtual returns (bool) {
        return interfaceId == 0x01ffc9a7;
    }
}

interface IBEP1155 {
    /**
     * @dev Emitted when `value` tokens of token type `id` are transferred from `from` to `to` by `operator`.
     */
    event TransferSingle(address indexed operator, address indexed from, address indexed to, uint256 id, uint256 value);

    /**
     * @dev Emitted when `operator` transfers `values` tokens of token type `id` to `to`.
     */
    event TransferBatch(address indexed operator, address indexed from, address indexed to, uint256[] ids, uint256[] values);

    /**
     * @dev Emitted when `account` grants or revokes `operator` approval to manage their tokens.
     */
    event ApprovalForAll(address indexed account, address indexed operator, bool approved);

    /**
     * @dev Emitted when the URI for token type `id` changes to `value`, if it is a non-fungible token.
     */
    event URI(string value, uint256 indexed id);

    /**
     * @dev Returns the amount of tokens of token type `id` owned by `account`.
     *
     * Requirements:
     * - `account` cannot be the zero address.
     */
    function balanceOf(address account, uint256 id) external view returns (uint256);

    /**
     * @dev Returns the amount of tokens of token type `id` owned by `account`.
     */
    function balanceOfBatch(address[] calldata accounts, uint256[] calldata ids) external view returns (uint256[] memory);

    /**
     * @dev Grants or revokes permission to `operator` to transfer the caller's tokens.
     *
     * See {setApprovalForAll}.
     *
     * Requirements:
     * - `operator` cannot be the caller.
     */
    function setApprovalForAll(address operator, bool approved) external;

    /**
     * @dev Returns true if `operator` is approved to transfer `account`'s tokens.
     *
     * See {setApprovalForAll}.
     */
    function isApprovedForAll(address account, address operator) external view returns (bool);

    /**
     * @dev Transfers `amount` tokens of token type `id` from `from` to `to`.
     *
     * Requirements:
     * - `to` cannot be the zero address.
     * - If `to` is a smart contract, it must implement {IBEP1155Receiver-onBEP1155Received}
     *   and return the acceptance magic value.
     */
    function safeTransferFrom(address from, address to, uint256 id, uint256 amount, bytes calldata data) external;

    /**
     * @dev Batch transfers `amount` tokens of token type `id` from `from` to `to`.
     *
     * Requirements:
     * - `to` cannot be the zero address.
     * - If `to` is a smart contract, it must implement {IBEP1155Receiver-onBEP1155BatchReceived}
     *   and return the acceptance magic value.
     */
    function safeBatchTransferFrom(address from, address to, uint256[] calldata ids, uint256[] calldata amounts, bytes calldata data) external;
}

interface IBEP1155Receiver is IERC165 {
    /**
     * @dev Handles the receipt of a single BEP1155 token type. This function is called at the end of a
     * `safeTransferFrom` after the transfer has been completed.
     *
     * NOTE: This function MUST return its function selector (0x2eb2a2d9) to conform to
     * {IBEP1155Receiver-onBEP1155Received}.
     * Calling this function in any other way WILL NOT work.
     */
    function onBEP1155Received(
        address operator,
        address from,
        uint256 id,
        uint256 amount,
        bytes calldata data
    ) external returns (bytes4);

    /**
     * @dev Handles the receipt of a multiple BEP1155 token types. This function
     * is called at the end of a `safeBatchTransferFrom` after the transfer has been
     * completed.
     *
     * NOTE: This function MUST return its function selector (0x9b9e5e03) to conform to
     * {IBEP1155Receiver-onBEP1155BatchReceived}.
     * Calling this function in any other way WILL NOT work.
     */
    function onBEP1155BatchReceived(
        address operator,
        address from,
        uint256[] calldata ids,
        uint256[] calldata amounts,
        bytes calldata data
    ) external returns (bytes4);
}

interface IERC165 {
    function supportsInterface(bytes4 interfaceId) external view returns (bool);
}

/**
 * @dev Implementation of the {IBEP1155} multi-token standard.
 */
contract BEP1155 is Context, ERC165, IBEP1155 {
    using Address for address;

    // Mapping from account to operator approvals
    mapping(address => mapping(address => bool)) private _operatorApprovals;

    // Mapping from token ID to account balances
    mapping(uint256 => mapping(address => uint256)) private _balances;

    // Optional mapping for token URIs
    mapping(uint256 => string) private _tokenURIs;

    // As the pattern, we use a base URI
    string private _baseURI;

    bytes4 private constant _BEP1155_RECEIVED = 0x2eb2a2d9;
    bytes4 private constant _BEP1155_BATCH_RECEIVED = 0x9b9e5e03;

    /**
     * @dev See {IERC165-supportsInterface}.
     */
    function supportsInterface(bytes4 interfaceId) public view virtual override(ERC165, IERC165) returns (bool) {
        return interfaceId == type(IBEP1155).interfaceId || super.supportsInterface(interfaceId);
    }

    /**
     * @dev See {IBEP1155-balanceOf}.
     *
     * Requirements:
     * - `account` cannot be the zero address.
     */
    function balanceOf(address account, uint256 id) public view virtual override returns (uint256) {
        require(account != address(0), "BEP1155: balance query for the zero address");
        return _balances[id][account];
    }

    /**
     * @dev See {IBEP1155-balanceOfBatch}.
     *
     * Requirements:
     * - `accounts` and `ids` must have the same length.
     */
    function balanceOfBatch(address[] calldata accounts, uint256[] calldata ids) public view virtual override returns (uint256[] memory) {
        require(accounts.length == ids.length, "BEP1155: accounts and ids length mismatch");

        uint256[] memory batchBalances = new uint256[](accounts.length);

        for (uint256 i = 0; i < accounts.length; ++i) {
            batchBalances[i] = balanceOf(accounts[i], ids[i]);
        }

        return batchBalances;
    }

    /**
     * @dev See {IBEP1155-setApprovalForAll}.
     */
    function setApprovalForAll(address operator, bool approved) public virtual override {
        _setApprovalForAll(_msgSender(), operator, approved);
    }

    /**
     * @dev See {IBEP1155-isApprovedForAll}.
     */
    function isApprovedForAll(address account, address operator) public view virtual override returns (bool) {
        return _operatorApprovals[account][operator];
    }

    /**
     * @dev See {IBEP1155-safeTransferFrom}.
     */
    function safeTransferFrom(
        address from,
        address to,
        uint256 id,
        uint256 amount,
        bytes calldata data
    ) public virtual override {
        require(from == _msgSender() || isApprovedForAll(from, _msgSender()), "BEP1155: caller is not owner nor approved");
        _safeTransfer(from, to, id, amount, data);
    }

    /**
     * @dev See {IBEP1155-safeBatchTransferFrom}.
     */
    function safeBatchTransferFrom(
        address from,
        address to,
        uint256[] calldata ids,
        uint256[] calldata amounts,
        bytes calldata data
    ) public virtual override {
        require(from == _msgSender() || isApprovedForAll(from, _msgSender()), "BEP1155: transfer caller is not owner nor approved");
        _safeBatchTransfer(from, to, ids, amounts, data);
    }

    /**
     * @dev Returns the URI for token type `id`.
     *
     * If the `\{id\}` substring is present in the URI, it must be replaced by
     * `{id}`.
     */
    function uri(uint256 id) public view virtual returns (string memory) {
        return _tokenURIs[id];
    }

    /**
     * @dev Sets the URI for a given token type.
     */
    function _setURI(uint256 id, string memory newuri) internal virtual {
        _tokenURIs[id] = newuri;
    }

    /**
     * @dev Sets the base URI for all token types.
     */
    function _setBaseURI(string memory newuri) internal virtual {
        _baseURI = newuri;
    }

    /**
     * @dev Returns the base URI.
     */
    function _baseURIInternal() internal view virtual returns (string memory) {
        return _baseURI;
    }

    /**
     * @dev Transfers `amount` tokens of token type `id` from `from` to `to`.
     *
     * Requirements:
     * - `to` cannot be the zero address.
     * - `from` must have a balance of at least `amount`.
     * - If `to` is a smart contract, it must implement the {IBEP1155Receiver}
     *   interface.
     */
    function _safeTransfer(
        address from,
        address to,
        uint256 id,
        uint256 amount,
        bytes calldata data
    ) internal virtual {
        require(to != address(0), "BEP1155: transfer to the zero address");

        address operator = _msgSender();
        uint256 fromBalance = _balances[id][from];

        require(fromBalance >= amount, "BEP1155: insufficient balance for transfer");
        unchecked {
            _balances[id][from] = fromBalance - amount;
            _balances[id][to] += amount;
        }

        emit TransferSingle(operator, from, to, id, amount);

        require(
            to.isContract() ? _checkOnBEP1155Received(operator, from, to, id, amount, data) : true,
            "BEP1155: transfer to non BEP1155Receiver implementer"
        );
    }

    /**
     * @dev Batch transfers `amount` tokens of token type `id` from `from` to `to`.
     *
     * Requirements:
     * - `to` cannot be the zero address.
     * - `from` must have a balance of at least `amount` for each token type.
     * - If `to` is a smart contract, it must implement the {IBEP1155Receiver}
     *   interface.
     */
    function _safeBatchTransfer(
        address from,
        address to,
        uint256[] calldata ids,
        uint256[] calldata amounts,
        bytes calldata data
    ) internal virtual {
        require(ids.length == amounts.length, "BEP1155: ids and amounts length mismatch");
        require(to != address(0), "BEP1155: transfer to the zero address");

        address operator = _msgSender();

        for (uint256 i = 0; i < ids.length; ++i) {
            uint256 id = ids[i];
            uint256 amount = amounts[i];

            uint256 fromBalance = _balances[id][from];
            require(fromBalance >= amount, "BEP1155: insufficient balance for transfer");
            unchecked {
                _balances[id][from] = fromBalance - amount;
                _balances[id][to] += amount;
            }
        }

        emit TransferBatch(operator, from, to, ids, amounts);

        require(
            to.isContract() ? _checkOnBEP1155BatchReceived(operator, from, to, ids, amounts, data) : true,
            "BEP1155: transfer to non BEP1155Receiver implementer"
        );
    }

    /**
     * @dev Approve or remove `operator` as an operator for the caller.
     *
     * Emits an {ApprovalForAll} event.
     */
    function _setApprovalForAll(address owner, address operator, bool approved) internal virtual {
        require(owner != operator, "BEP1155: setting approval status for self");
        _operatorApprovals[owner][operator] = approved;
        emit ApprovalForAll(owner, operator, approved);
    }

    /**
     * @dev Check if receiver implements BEP1155Receiver interface.
     */
    function _checkOnBEP1155Received(
        address operator,
        address from,
        address to,
        uint256 id,
        uint256 amount,
        bytes calldata data
    ) private returns (bool) {
        bytes4 retval = IBEP1155Receiver(to).onBEP1155Received(operator, from, id, amount, data);
        return (retval == _BEP1155_RECEIVED);
    }

    /**
     * @dev Check if receiver implements BEP1155Receiver interface for batch.
     */
    function _checkOnBEP1155BatchReceived(
        address operator,
        address from,
        address to,
        uint256[] calldata ids,
        uint256[] calldata amounts,
        bytes calldata data
    ) private returns (bool) {
        bytes4 retval = IBEP1155Receiver(to).onBEP1155BatchReceived(operator, from, ids, amounts, data);
        return (retval == _BEP1155_BATCH_RECEIVED);
    }

    /**
     * @dev Creates `amount` tokens of token type `id`, and assigns them to `to`.
     *
     * Requirements:
     * - `to` cannot be the zero address.
     * - If `to` refers to a smart contract, it must implement {IBEP1155Receiver-onBEP1155Received}
     *   and return the acceptance magic value.
     */
    function _mint(address to, uint256 id, uint256 amount, bytes memory data) internal virtual {
        require(to != address(0), "BEP1155: mint to the zero address");

        address operator = _msgSender();

        _balances[id][to] += amount;
        emit TransferSingle(operator, address(0), to, id, amount);

        require(
            to.isContract() ? _checkOnBEP1155Received(operator, address(0), to, id, amount, data) : true,
            "BEP1155: transfer to non BEP1155Receiver implementer"
        );
    }

    /**
     * @dev Batch version of {_mint}.
     */
    function _mintBatch(address to, uint256[] memory ids, uint256[] memory amounts, bytes memory data) internal virtual {
        require(to != address(0), "BEP1155: mint to the zero address");
        require(ids.length == amounts.length, "BEP1155: ids and amounts length mismatch");

        address operator = _msgSender();

        for (uint256 i = 0; i < ids.length; i++) {
            _balances[ids[i]][to] += amounts[i];
        }

        emit TransferBatch(operator, address(0), to, ids, amounts);

        require(
            to.isContract() ? _checkOnBEP1155BatchReceived(operator, address(0), to, ids, amounts, data) : true,
            "BEP1155: transfer to non BEP1155Receiver implementer"
        );
    }

    /**
     * @dev Destroys `amount` tokens of token type `id` from `from`
     *
     * Requirements:
     * - `from` must have a balance of at least `amount`.
     */
    function _burn(address from, uint256 id, uint256 amount) internal virtual {
        require(from != address(0), "BEP1155: burn from the zero address");

        address operator = _msgSender();
        uint256 fromBalance = _balances[id][from];

        require(fromBalance >= amount, "BEP1155: burn amount exceeds balance");
        unchecked {
            _balances[id][from] = fromBalance - amount;
        }

        emit TransferSingle(operator, from, address(0), id, amount);
    }

    /**
     * @dev Batch version of {_burn}.
     */
    function _burnBatch(address from, uint256[] memory ids, uint256[] memory amounts) internal virtual {
        require(from != address(0), "BEP1155: burn from the zero address");
        require(ids.length == amounts.length, "BEP1155: ids and amounts length mismatch");

        address operator = _msgSender();

        for (uint256 i = 0; i < ids.length; i++) {
            uint256 id = ids[i];
            uint256 amount = amounts[i];
            uint256 fromBalance = _balances[id][from];
            require(fromBalance >= amount, "BEP1155: burn amount exceeds balance");
            unchecked {
                _balances[id][from] = fromBalance - amount;
            }
        }

        emit TransferBatch(operator, from, address(0), ids, amounts);
    }
}

/**
 * @dev Contract module which provides a basic access control mechanism, where
 * there is an account (an owner) that can be granted exclusive access to
 * specific functions.
 */
abstract contract Ownable is Context {
    address private _owner;

    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    /**
     * @dev Initializes the contract setting the deployer as the initial owner.
     */
    constructor() {
        _transferOwnership(_msgSender());
    }

    /**
     * @dev Returns the address of the current owner.
     */
    function owner() public view virtual returns (address) {
        return _owner;
    }

    /**
     * @dev Throws if called by any account other than the owner.
     */
    modifier onlyOwner() {
        require(owner() == _msgSender(), "Ownable: caller is not the owner");
        _;
    }

    /**
     * @dev Leaves the contract without owner. It will not be possible to call
     * `onlyOwner` functions anymore. Can only be called by the current owner.
     */
    function renounceOwnership() public virtual onlyOwner {
        _transferOwnership(address(0));
    }

    /**
     * @dev Transfers ownership of the contract to a new account (`newOwner`)
     * and deletes any pending owner. Internal function without access restriction.
     */
    function _transferOwnership(address newOwner) internal virtual {
        address oldOwner = _owner;
        _owner = newOwner;
        emit OwnershipTransferred(oldOwner, newOwner);
    }
}

/**
 * @title BEP1155URIStorage
 * @dev BEP1155 token with storage based token URI management
 */
abstract contract BEP1155URIStorage is BEP1155 {
    using Strings for uint256;

    // Optional mapping for token URIs under the base URI
    string private _baseURIExtended;

    /**
     * @dev See {IERC721Metadata-tokenURI}.
     */
    function uri(uint256 tokenId) public view virtual override returns (string memory) {
        if (bytes(_baseURIExtended).length > 0) {
            return string(abi.encodePacked(_baseURIExtended, tokenId.toString()));
        }
        return super.uri(tokenId);
    }

    /**
     * @dev Sets the base URI for all token types.
     */
    function setBaseURI(string memory newBaseURI) external onlyOwner {
        _baseURIExtended = newBaseURI;
    }
}

/**
 * @dev Library for reading and writing string pointers to byte arrays.
 */
library Strings {
    bytes private constant ALPHABET0123456789 = "0123456789abcdefghijklmnopqrstuvwxyz";

    /**
     * @dev Converts a `uint256` to its ASCII `string` decimal representation.
     */
    function toString(uint256 value) internal pure returns (string memory) {
        // Currently based on openzeppelin implementation
        if (value == 0) {
            return "0";
        }
        uint256 temp = value;
        uint256 digits;
        while (temp != 0) {
            digits++;
            temp /= 10;
        }
        bytes memory buffer = new bytes(digits);
        while (value != 0) {
            digits -= 1;
            buffer[digits] = bytes1(uint8(48 + (value % 10)));
            value /= 10;
        }
        return string(buffer);
    }
}

/**
 * Collection of functions related to address type
 */
library Address {
    /**
     * @dev Returns true if `account` is a contract.
     *
     * [IMPORTANT]
     * ====
     * It is unsafe to assume that code executed at an address can be assumed to never 
     * contain malicious code, but this is impossible in the general case in  Solidity.
     *
     * You should ensure that the call is not to a malicious contract:
     * https://medium.com/coinmonks/solidity-why-do-we-need-eth-addr-f8fcc2f3b8a1
     * ====
     */
    function isContract(address account) internal view returns (bool) {
        // This method relies on extcodesize, which returns 0 for contracts in
        // construction, since the code is not yet stored when deploying.
        uint256 size;
        assembly {
            size := extcodesize(account)
        }
        return size > 0;
    }
}

/**
 * BEP1155 Token with minting and burning capabilities.
 */
contract BEP1155Token is BEP1155URIStorage, Ownable {
    /**
     * @dev Creates a new token type and assigns the initial supply to the caller.
     */
    function create(uint256 initialSupply, string memory _tokenURI) external onlyOwner returns (uint256 id) {
        id = 1; // Simplified - in production would use next token ID
        _mint(_msgSender(), id, initialSupply, "");
        _setURI(id, _tokenURI);
    }

    /**
     * @dev Mints `amount` tokens of token type `id` to `to`.
     */
    function mint(address to, uint256 id, uint256 amount) external onlyOwner {
        _mint(to, id, amount, "");
    }

    /**
     * @dev Burns `amount` tokens of token type `id` from `from`.
     */
    function burn(address from, uint256 id, uint256 amount) external onlyOwner {
        _burn(from, id, amount);
    }
}