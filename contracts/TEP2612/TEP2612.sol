// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title TEP2612 Permit Token Standard
/// @notice Gasless token transfers using EIP-712 permit signature
/// @dev Based on EIP-2612

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/utils/cryptography/SignatureVerification.sol";

/// @notice TEP2612 permit interface
interface ITEP2612 {
    /// @notice Returns the current nonce for an owner
    function nonces(address owner) external view returns (uint256);

    /// @notice Returns the domain separator for permit
    function DOMAIN_SEPARATOR() external view returns (bytes32);

    /// @notice Event emitted when permit is used
    event ApprovalUsed(
        address indexed owner,
        address indexed spender,
        uint256 value,
        uint256 nonce,
        bytes32 hash
    );
}

/// @title TEP2612 Permit Token
/// @dev Token with permit() function for gasless approvals
contract TEP2612Token is ERC20, ITEP2612 {
    bytes32 public constant PERMIT_TYPEHASH =
        keccak256(
            "Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)"
        );

    mapping(address => uint256) public nonces;

    bytes32 public override(DOMAIN_SEPARATOR) domainSeparator;

    constructor(
        string memory name,
        string memory symbol,
        uint256 chainId_
    ) ERC20(name, symbol) {
        // Note: this is calculated at runtime in production
        domainSeparator = keccak256(
            abi.encode(
                keccak256(
                    "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
                ),
                keccak256(bytes(name)),
                keccak256(bytes("1")),
                chainId_,
                address(this)
            )
        );
    }

    /// @notice Allows a owner to approve a spender via a permit signature
    /// @param owner Token owner
    /// @param spender Spender address
    /// @param value Approval amount
    /// @param deadline Deadline timestamp
    /// @param v Signature v
    /// @param r Signature r
    /// @param s Signature s
    function permit(
        address owner,
        address spender,
        uint256 value,
        uint256 deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external {
        require(deadline >= block.timestamp, "TEP2612: expired deadline");

        bytes32 structHash = keccak256(
            abi.encode(
                PERMIT_TYPEHASH,
                owner,
                spender,
                value,
                nonces[owner],
                deadline
            )
        );

        bytes32 hash = keccak256(
            abi.encodePacked("\x19\x01", domainSeparator, structHash)
        );

        // Recover signer
        address signer = ecrecover(hash, v, r, s);
        require(signer == owner, "TEP2612: invalid signature");

        // Update nonce
        nonces[owner]++;

        // Approve
        _approve(owner, spender, value);

        emit ApprovalUsed(owner, spender, value, nonces[owner], hash);
    }

    /// @notice Returns the domain separator
    function DOMAIN_SEPARATOR() external view returns (bytes32) {
        return domainSeparator;
    }

    /// @notice Returns the current nonce for an owner
    function nonces(address owner) external view returns (uint256) {
        return nonces[owner];
    }

    // Optional: Add permitTransfer for gasless transfers
    /// @notice Gasless transfer using permit
    /// @param from Source address
    /// @param to Destination address
    /// @param amount Amount to transfer
    /// @param deadline Deadline timestamp
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
        require(deadline >= block.timestamp, "TEP2612: expired deadline");

        // Build permit for transfer
        bytes32 structHash = keccak256(
            abi.encode(
                keccak256(
                    "PermitTransfer(address from,address to,uint256 amount,uint256 nonce,uint256 deadline)"
                ),
                from,
                to,
                amount,
                nonces[from],
                deadline
            )
        );

        bytes32 hash = keccak256(
            abi.encodePacked("\x19\x01", domainSeparator, structHash)
        );

        // Recover signer
        address signer = ecrecover(hash, v, r, s);
        require(signer == from, "TEP2612: invalid signature");

        // Update nonce
        nonces[from]++;

        // Transfer
        _transfer(from, to, amount);
    }
}

/// @title Token with Permit - Helper
/// @dev Helper to enable gasless transfers
contract PermitHelper {
    /// @notice Build permit data for EIP-712 signing
    /// @param token Token address
    /// @param owner Owner address
    /// @param spender Spender address
    /// @param value Approval amount
    /// @param nonce Nonce
    /// @param deadline Deadline
    /// @param chainId Chain ID
    /// @return data The encoded permit data
    function buildPermitData(
        address token,
        address owner,
        address spender,
        uint256 value,
        uint256 nonce,
        uint256 deadline,
        uint256 chainId
    ) external view returns (bytes memory) {
        bytes32 domainSeparator = keccak256(
            abi.encode(
                keccak256(
                    "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
                ),
                keccak256(bytes("TEP2612 Token")),
                keccak256(bytes("1")),
                chainId,
                token
            )
        );

        bytes32 structHash = keccak256(
            abi.encode(
                keccak256(
                    "Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)"
                ),
                owner,
                spender,
                value,
                nonce,
                deadline
            )
        );

        return
            abi.encodePacked(
                bytes1(0x19),
                bytes1(0x01),
                domainSeparator,
                structHash
            );
    }
}