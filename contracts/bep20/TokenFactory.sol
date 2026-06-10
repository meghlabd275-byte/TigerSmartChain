// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title TokenFactory
 * @dev Token factory for creating BEP-20 tokens
 * @notice Allows permissionless creation of new tokens
 */
contract TokenFactory {
    // =============================================================================
    // STATE VARIABLES
    // =============================================================================

    // Owner
    address public owner;

    // Token templates
    mapping(uint256 => address) public tokenTemplates;

    // Created tokens
    address[] public createdTokens;

    // Token template count
    uint256 public templateCount;

    // Events
    event TokenCreated(
        address indexed token,
        address indexed owner,
        string name,
        string symbol,
        uint8 decimals
    );
    event TemplateRegistered(uint256 indexed templateId, address template);

    // Errors
    error Unauthorized();
    error InvalidTemplate();
    error DeploymentFailed();

    // =============================================================================
    // MODIFIERS
    // =============================================================================

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    // =============================================================================
    // CONSTRUCTOR
    // =============================================================================

    constructor() {
        owner = msg.sender;
        
        // Register default templates
        _registerTemplate(address(new TokenV1()));
    }

    // =============================================================================
    // TOKEN CREATION
    // =============================================================================

    /**
     * @notice Create a new token
     * @param templateId Template ID
     * @param name Token name
     * @param symbol Token symbol
     * @param decimals Token decimals
     * @param initialSupply Initial token supply
     * @return tokenAddress Address of created token
     */
    function createToken(
        uint256 templateId,
        string calldata name,
        string calldata symbol,
        uint8 decimals,
        uint256 initialSupply
    ) external returns (address tokenAddress) {
        // Get template
        address template = tokenTemplates[templateId];
        if (template == address(0)) revert InvalidTemplate();

        // Deploy token using template
        TokenV1 token = TokenV1(_cloneTemplate(template));
        
        // Initialize token
        token.initialize(
            name,
            symbol,
            decimals,
            initialSupply,
            msg.sender
        );

        tokenAddress = address(token);
        createdTokens.push(tokenAddress);

        emit TokenCreated(
            tokenAddress,
            msg.sender,
            name,
            symbol,
            decimals
        );
    }

    /**
     * @notice Create a token with custom parameters
     * @param templateId Template ID
     * @param data Custom initialization data
     * @return tokenAddress Address of created token
     */
    function createTokenCustom(
        uint256 templateId,
        bytes calldata data
    ) external returns (address tokenAddress) {
        address template = tokenTemplates[templateId];
        if (template == address(0)) revert InvalidTemplate();

        TokenV1 token = TokenV1(_cloneTemplate(template));
        
        // Custom initialization
        token.initializeCustom(msg.sender, data);

        tokenAddress = address(token);
        createdTokens.push(tokenAddress);
    }

    // =============================================================================
    // TEMPLATE MANAGEMENT
    // =============================================================================

    /**
     * @notice Register a new token template
     * @param template Template address
     * @return templateId Registered template ID
     */
    function registerTemplate(address template) external onlyOwner returns (uint256 templateId) {
        templateId = _registerTemplate(template);
    }

    function _registerTemplate(address template) internal returns (uint256 templateId) {
        templateId = templateCount++;
        tokenTemplates[templateId] = template;
        emit TemplateRegistered(templateId, template);
    }

    // =============================================================================
    // CLONING
    // =============================================================================

    /**
     * @notice Clone a token template
     * @param template Template address
     * @return cloned address
     */
    function _cloneTemplate(address template) internal returns (address) {
        // Minimal proxy clone
        bytes32 salt = keccak256(abi.encodePacked(template, block.timestamp));
        
        address cloned;
        assembly {
            let ptr := mload(0x40)
            
            mstore(ptr, 0x3d602d80600a3d3981f3363d3d373d3d3d602d80600a3d3981f3363d3d373d3d3d5af43d82803e903d91602a57fd5bf3)
            mstore(add(ptr, 0x0d), shl(0x68, template))
            mstore(add(ptr, 0x0d), shl(0x80, salt))
            
            cloned := create2(0, ptr, 0x13, salt)
        }
        
        if (cloned == address(0)) revert DeploymentFailed();
        return cloned;
    }

    // =============================================================================
    // QUERY FUNCTIONS
    // =============================================================================

    /**
     * @notice Get number of created tokens
     * @return count Number of tokens
     */
    function getTokenCount() external view returns (uint256 count) {
        return createdTokens.length;
    }

    /**
     * @notice Get created tokens
     * @param start Start index
     * @param count Number of tokens
     * @return Array of token addresses
     */
    function getTokens(uint256 start, uint256 count)
        external
        view
        returns (address[] memory tokens)
    {
        tokens = new address[](count);
        for (uint256 i = 0; i < count; i++) {
            tokens[i] = createdTokens[start + i];
        }
    }
}

// =============================================================================
// TOKEN V1 TEMPLATE
// =============================================================================

/**
 * @title TokenV1
 * @dev Basic BEP-20 token implementation
 */
contract TokenV1 {
    // Token state
    string public name;
    string public symbol;
    uint8 public decimals;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    mapping(address => bool) public minters;

    // Owner
    address public owner;

    // Events
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event Mint(address indexed to, uint256 amount);

    // Constructor
    constructor() {
        owner = msg.sender;
        minters[msg.sender] = true;
    }

    /**
     * @notice Initialize token
     * @param _name Token name
     * @param _symbol Token symbol
     * @param _decimals Token decimals
     * @param _initialSupply Initial supply
     * @param _owner Token owner
     */
    function initialize(
        string calldata _name,
        string calldata _symbol,
        uint8 _decimals,
        uint256 _initialSupply,
        address _owner
    ) external {
        require(name.length == 0, "already initialized");
        
        name = _name;
        symbol = _symbol;
        decimals = _decimals;
        totalSupply = _initialSupply;
        balanceOf[_owner] = _initialSupply;
        
        emit Transfer(address(0), _owner, _initialSupply);
    }

    /**
     * @notice Custom initialization
     * @param _owner Owner address
     * @param data Custom data
     */
    function initializeCustom(address _owner, bytes calldata data) external {
        require(name.length == 0, "already initialized");
        
        // Decode custom data
        (name, symbol, decimals, totalSupply) = abi.decode(
            data,
            (string, string, uint8, uint256)
        );
        
        balanceOf[_owner] = totalSupply;
        emit Transfer(address(0), _owner, totalSupply);
    }

    /**
     * @notice Transfer tokens
     * @param to Recipient address
     * @param amount Amount to transfer
     * @return success Success
     */
    function transfer(address to, uint256 amount) external returns (bool success) {
        require(balanceOf[msg.sender] >= amount, "insufficient balance");
        
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        
        emit Transfer(msg.sender, to, amount);
        return true;
    }

    /**
     * @notice Approve tokens
     * @param spender Spender address
     * @param amount Amount to approve
     * @return success Success
     */
    function approve(address spender, uint256 amount) external returns (bool success) {
        allowance[msg.sender][spender] = amount;
        
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    /**
     * @notice Transfer from
     * @param from From address
     * @param to To address
     * @param amount Amount
     * @return success Success
     */
    function transferFrom(
        address from,
        address to,
        uint256 amount
    ) external returns (bool success) {
        require(balanceOf[from] >= amount, "insufficient balance");
        require(allowance[from][msg.sender] >= amount, "insufficient allowance");
        
        allowance[from][msg.sender] -= amount;
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        
        emit Transfer(from, to, amount);
        return true;
    }

    /**
     * @notice Mint new tokens
     * @param to Recipient
     * @param amount Amount to mint
     */
    function mint(address to, uint256 amount) external {
        require(minters[msg.sender], "not minter");
        
        totalSupply += amount;
        balanceOf[to] += amount;
        
        emit Mint(to, amount);
        emit Transfer(address(0), to, amount);
    }
}