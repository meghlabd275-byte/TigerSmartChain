// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title TEP165 Token Interface Detection Standard
/// @notice Standard for detecting if a contract implements a given interface
/// @dev Based on EIP-165

/// @notice Interface for TEP165
interface ITEP165 {
    /// @notice Returns true if this contract implements the interface
    /// @param interfaceId The interface identifier
    /// @return true if the contract implements the interface
    function supportsInterface(bytes4 interfaceId) external view returns (bool);
}

/// @title TEP165 Support
/// @dev Implementation of TEP165
abstract contract TEP165 is ITEP165 {
    /// @notice Maps interfaceId to whether the interface is supported
    mapping(bytes4 => bool) private _supportedInterfaces;

    /// @notice Interface IDs for common token standards
    bytes4 public constant ITEP20_ID = 0x06ef2bec; // type(ITEP20).interfaceId
    bytes4 public constant ITEP721_ID = 0x80ac58cd; // type(ITEP721).interfaceId
    bytes4 public constant ITEP1155_ID = 0xd9b67a26; // type(ITEP1155).interfaceId

    /// @notice Constructor registers the supported interfaces
    constructor() {
        _registerInterface(0x01ffc9a7); // ITEP165 itself
        _registerInterface(0x06ef2bec); // ITEP20
        _registerInterface(0x80ac58cd); // ITEP721
        _registerInterface(0xd9b67a26); // ITEP1155
    }

    /// @notice See ITEP165.supportsInterface
    function supportsInterface(bytes4 interfaceId)
        public
        view
        virtual
        override
        returns (bool)
    {
        return _supportedInterfaces[interfaceId];
    }

    /// @notice Registers an interface
    /// @param interfaceId Interface to register
    function _registerInterface(bytes4 interfaceId) internal {
        require(interfaceId != 0xffffffff, "TEP165: invalid interface");
        _supportedInterfaces[interfaceId] = true;
    }

    /// @notice Helper to check if an address is a contract
    function isContract(address account) internal view returns (bool) {
        uint256 size;
        assembly {
            size := extcodesize(account)
        }
        return size > 0;
    }
}

/// @title Token Interface Detector
/// @dev Utility contract to detect token interface standards
contract TokenInterfaceDetector {
    /// @notice Detects if an address supports TEP20
    /// @param account Address to check
    /// @return true if the address supports TEP20
    function supportsTEP20(address account) public view returns (bool) {
        (bool success, bytes memory result) = account.staticcall(
            abi.encodeCall(ITEP165.supportsInterface, (ITEP20_ID))
        );
        return success && result.length > 0 && abi.decode(result, (bool));
    }

    /// @notice Detects if an address supports TEP721
    /// @param account Address to check
    /// @return true if the address supports TEP721
    function supportsTEP721(address account) public view returns (bool) {
        (bool success, bytes memory result) = account.staticcall(
            abi.encodeCall(ITEP165.supportsInterface, (ITEP721_ID))
        );
        return success && result.length > 0 && abi.decode(result, (bool));
    }

    /// @notice Detects if an address supports TEP1155
    /// @param account Address to check
    /// @return true if the address supports TEP1155
    function supportsTEP1155(address account) public view returns (bool) {
        (bool success, bytes memory result) = account.staticcall(
            abi.encodeCall(ITEP165.supportsInterface, (ITEP1155_ID))
        );
        return success && result.length > 0 && abi.decode(result, (bool));
    }

    /// @notice Detects the token standard
    /// @param account Address to check
    /// @return 20 for TEP20, 721 for TEP721, 1155 for TEP1155, 0 for unknown
    function detectTokenStandard(address account) public view returns (uint256) {
        if (supportsTEP20(account)) return 20;
        if (supportsTEP721(account)) return 721;
        if (supportsTEP1155(account)) return 1155;
        return 0;
    }

    // Type-safe interface IDs
    bytes4 internal constant ITEP20_ID = 0x06ef2bec;
    bytes4 internal constant ITEP721_ID = 0x80ac58cd;
    bytes4 internal constant ITEP1155_ID = 0xd9b67a26;
}