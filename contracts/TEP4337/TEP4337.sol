// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title TEP4337 Account Abstraction Standard
/// @notice Smart contract wallet standard with native account abstraction
/// @dev Based on EIP-4337

/// @notice Entry point interface
interface IEntryPoint {
    function handleOps(
        bytes[] calldata,
        address payable,
        uint256
    ) external;

    function getUserOpHash(UserOperation calldata) external view returns (bytes32);

    function getDeposit(address) external view returns (uint256);

    function addDepositTo(address) external payable;
}

/// @notice User operation structure
struct UserOperation {
    address sender;
    uint256 nonce;
    bytes initCode;
    bytes callData;
    uint256 callGasLimit;
    uint256 verificationGasLimit;
    uint256 preVerificationGas;
    uint256 maxFeePerGas;
    uint256 maxPriorityFeePerGas;
    bytes paymasterAndData;
    bytes signature;
}

/// @notice Wallet interface
interface IWallet {
    /// @notice Validate user operation
    /// @param userOp User operation
    /// @param userOpHash Hash of user operation
    /// @return validationTime Valid until timestamp
    /// @return requiredPrefund Required prefund
    function validateUserOp(
        UserOperation calldata userOp,
        bytes32 userOpHash,
        uint256
    ) external returns (uint256, uint256);

    /// @notice Execute user operation
    /// @param callData Call data
    function execute(bytes calldata callData) external payable;

    /// @notice Execute multiple calls
    /// @param calls Array of calls
    function executeBatch(bytes[] calldata calls) external payable;
}

/// @title Simple Wallet
/// @dev Simple smart contract wallet implementing TEP4337
contract SimpleWallet is IWallet {
    // Entry point contract
    IEntryPoint public immutable entryPoint;

    // Owner address
    address public owner;

    // Nonce for replay protection
    uint256 public nonce;

    // Deposit in the entry point
    mapping(address => uint256) public deposits;

    /// @notice Constructor
    /// @param entryPoint_ Entry point contract address
    /// @param owner_ Owner address
    constructor(address entryPoint_, address owner_) {
        require(entryPoint_ != address(0), "invalid entry point");
        require(owner_ != address(0), "invalid owner");

        entryPoint = IEntryPoint(entryPoint_);
        owner = owner_;
    }

    /// @notice Validate user operation
    function validateUserOp(
        UserOperation calldata userOp,
        bytes32 userOpHash,
        uint256 missingFunds
    ) external returns (uint256, uint256) {
        require(msg.sender == address(entryPoint), "not entry point");

        // Check nonce
        require(userOp.nonce == nonce, "invalid nonce");

        // Check signature
        bytes32 hash = keccak256(
            abi.encodePacked(
                "\x19\x01",
                entryPoint.getUserOpHash(userOp),
                address(this)
            )
        );

        require(
            _validateSignature(hash, userOp.signature),
            "invalid signature"
        );

        // Increment nonce
        nonce++;

        // Return validation time and required prefund
        return (block.timestamp + 300, missingFunds);
    }

    /// @notice Execute call data
    function execute(bytes calldata callData) external override payable {
        require(msg.sender == address(entryPoint), "not entry point");

        // Execute call
        (bool success, ) = address(this).delegatecall(callData);
        require(success, "execution failed");
    }

    /// @notice Execute multiple calls
    function executeBatch(bytes[] calldata calls) external override payable {
        require(msg.sender == address(entryPoint), "not entry point");

        for (uint256 i = 0; i < calls.length; i++) {
            (bool success, ) = address(this).delegatecall(calls[i]);
            require(success, "call failed");
        }
    }

    /// @notice Execute internal call
    function _execute(address to, bytes calldata data)
        internal
        returns (bool success, bytes memory result)
    {
        (success, result) = to.call{value: 0}(data);
    }

    /// @notice Validate signature
    function _validateSignature(bytes32 hash, bytes memory signature)
        internal
        view
        returns (bool)
    {
        // Check signature length
        if (signature.length == 65) {
            // Split signature
            bytes32 r;
            bytes32 s;
            uint8 v;

            assembly {
                r := mload(add(signature, 0x20))
                s := mload(add(signature, 0x40))
                v := byte(0, mload(add(signature, 0x60)))
            }

            // Recover signer
            address signer = ecrecover(hash, v, r, s);
            return signer == owner;
        }

        // Direct match for owner
        return keccak256(signature) == keccak256(abi.encodePacked(owner));
    }

    /// @notice Receive deposits
    receive() external payable {
        // Deposit to entry point
        if (msg.value > 0) {
            entryPoint.addDepositTo{value: msg.value}(address(this));
        }
    }

    /// @notice Withdraw from deposit
    function withdrawDeposit(uint256 amount, address payable recipient) external {
        require(msg.sender == owner, "not owner");

        uint256 available = entryPoint.getDeposit(address(this));
        require(available >= amount, "insufficient deposit");

        entryPoint.addDepositTo{value: 0}(address(0)); // Reduce deposit
        recipient.transfer(amount);
    }

    /// @notice Transfer ownership
    function transferOwnership(address newOwner) external {
        require(msg.sender == owner, "not owner");
        require(newOwner != address(0), "invalid owner");

        owner = newOwner;
    }
}

/// @title Entry Point
/// @dev Entry point contract for account abstraction
contract EntryPoint is IEntryPoint {
    // User operation hash
    mapping(bytes32 => uint256) private userOpHashes;

    // Deposits
    mapping(address => uint256) private deposits;

    // Revert reason
    error FailedOp(uint256 opIndex, string reason);

    /// @notice Handle user operations
    function handleOps(
        bytes[] calldata ops,
        address payable,
        uint256
    ) external override {
        for (uint256 i = 0; i < ops.length; i++) {
            UserOperation memory op = abi.decode(ops[i], (UserOperation));

            // Get user op hash
            bytes32 opHash = getUserOpHash(op);
            userOpHashes[opHash] = block.timestamp;

            // Validate and execute
            _handleOp(op, opHash);
        }
    }

    /// @notice Get user operation hash
    function getUserOpHash(UserOperation calldata op)
        external
        view
        override
        returns (bytes32)
    {
        return
            keccak256(
                abi.encode(
                    op.sender,
                    op.nonce,
                    keccak256(op.initCode),
                    keccak256(op.callData),
                    op.callGasLimit,
                    op.verificationGasLimit,
                    op.preVerificationGas,
                    op.maxFeePerGas,
                    op.maxPriorityFeePerGas,
                    keccak256(op.paymasterAndData),
                    block.chainid
                )
            );
    }

    /// @notice Get deposit
    function getDeposit(address account)
        external
        view
        override
        returns (uint256)
    {
        return deposits[account];
    }

    /// @notice Add deposit
    function addDepositTo(address) external payable override {
        deposits[msg.sender] += msg.value;
    }

    /// @notice Handle single operation
    function _handleOp(UserOperation memory op, bytes32 opHash) internal {
        // Check init code
        if (op.initCode.length > 0) {
            // Deploy wallet via init code
            address wallet;
            assembly {
                wallet := create2(
                    0,
                    add(op.initCode, 0x20),
                    mload(op.initCode),
                    op.nonce
                )
            }
            require(wallet != address(0), "init failed");
        }

        // Require sufficient gas
        require(
            gasleft() >= op.verificationGasLimit + op.callGasLimit,
            "insufficient gas"
        );

        // Call wallet
        try
            IWallet(op.sender).validateUserOp(
                op,
                opHash,
                op.verificationGasLimit
            )
        returns (uint256, uint256) {
            // Success
        } catch {
            // Revert
            revert FailedOp(0, "validation failed");
        }

        // Execute call data
        if (op.callData.length > 0) {
            (bool success, ) = op.sender.call{value: 0}(op.callData);
            require(success, "call failed");
        }
    }
}

/// @notice Paymaster interface
interface IPaymaster {
    function validatePaymasterUserOp(
        UserOperation calldata,
        bytes32,
        uint256
    ) external returns (bytes memory, uint256);

    function postOp(
        bytes calldata,
        UserOperation calldata,
        uint256,
        bytes calldata,
        uint256
    ) external payable;
}

/// @title Gas Sponsor
/// @dev Paymaster that sponsors gas for certain users
contract GasSponsor is IPaymaster {
    // Supported token for payment
    address public supportedToken;

    // Exchange rate (token to native)
    uint256 public exchangeRate;

    mapping(address => bool) public authorizedSenders;

    /// @notice Constructor
    /// @param _token Supported token
    /// @param _exchangeRate Exchange rate
    constructor(address _token, uint256 _exchangeRate) {
        supportedToken = _token;
        exchangeRate = _exchangeRate;
    }

    /// @notice Validate paymaster user operation
    function validatePaymasterUserOp(
        UserOperation calldata,
        bytes32,
        uint256
    ) external pure override returns (bytes memory, uint256) {
        // Accept all - in production, add whitelist
        return ("", 0);
    }

    /// @notice Post operation hook
    function postOp(
        bytes calldata,
        UserOperation calldata,
        uint256,
        bytes calldata,
        uint256
    ) external payable override {
        // Handle post-operation logic
    }

    /// @notice Authorize a sender
    function authorizeSender(address sender) external {
        authorizedSenders[sender] = true;
    }
}