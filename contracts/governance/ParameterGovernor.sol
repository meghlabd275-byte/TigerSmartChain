// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title ParameterGovernor
 * @dev Dynamic parameter governance for protocol parameters
 * @notice Allows on-chain governance of protocol parameters
 */
contract ParameterGovernor {
    // =============================================================================
    // DATA STRUCTURES
    // =============================================================================

    // Parameter structure
    struct Parameter {
        bytes32 name;
        uint256 value;
        uint256 minValue;
        uint256 maxValue;
        uint256 lastUpdate;
        address proposer;
        uint256 proposalTime;
    }

    // Proposal for parameter change
    struct Proposal {
        bytes32 paramName;
        uint256 newValue;
        uint256 votes;
        uint256 startTime;
        uint256 endTime;
        bool executed;
        mapping(address => bool) voters;
    }

    // =============================================================================
    // STATE VARIABLES
    // =============================================================================

    // Owner
    address public owner;

    // Parameters
    mapping(bytes32 => Parameter) public parameters;

    // Proposals
    mapping(bytes32 => Proposal) public proposals;

    // Active proposals
    bytes32[] public activeProposals;

    // Voting configuration
    uint256 public votingPeriod; // Duration of voting
    uint256 public votingThreshold; // Minimum votes to pass
    uint256 public executionDelay; // Delay before execution after vote

    // Total voting power
    uint256 public totalVotingPower;

    // Voter voting power
    mapping(address => uint256) public votingPower;

    // Events
    event ParameterProposed(
        bytes32 indexed name,
        uint256 newValue,
        address indexed proposer
    );
    event ParameterUpdated(
        bytes32 indexed name,
        uint256 oldValue,
        uint256 newValue
    );
    event VoteCast(
        bytes32 indexed proposal,
        address indexed voter,
        uint256 weight
    );
    event ProposalExecuted(bytes32 indexed proposal);
    event VotingPowerUpdated(address indexed account, uint256 power);

    // Errors
    error Unauthorized();
    error InvalidValue();
    error ProposalNotFound();
    error VotingNotActive();
    error AlreadyVoted();
    error ProposalAlreadyExecuted();
    error ExecutionNotReady();

    // =============================================================================
    // MODIFIERS
    // =============================================================================

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    modifier proposalActive(bytes32 proposalId) {
        if (!proposals[proposalId].executed) {
            Proposal storage p = proposals[proposalId];
            if (block.timestamp < p.startTime || block.timestamp > p.endTime) {
                revert VotingNotActive();
            }
        } else {
            revert ProposalAlreadyExecuted();
        }
        _;
    }

    // =============================================================================
    // CONSTRUCTOR
    // =============================================================================

    constructor() {
        owner = msg.sender;
        votingPeriod = 3 days;
        votingThreshold = 100 ether;
        executionDelay = 1 days;

        // Initialize default parameters
        _setParameter("MIN_STAKE", 100 ether, 1 ether, 10000 ether);
        _setParameter("MAX_VALIDATORS", 50, 1, 100);
        _setParameter("COMMISSION_RATE", 1000, 0, 10000); // in basis points
        _setParameter("SLASH_RATE", 1000, 0, 10000); // 10% default
        _setParameter("UNBONDING_PERIOD", 7 days, 1 days, 30 days);
    }

    // =============================================================================
    // PARAMETER MANAGEMENT
    // =============================================================================

    /**
     * @notice Set a parameter
     * @param name Parameter name
     * @param value Parameter value
     * @param min Minimum value
     * @param max Maximum value
     */
    function _setParameter(
        bytes32 name,
        uint256 value,
        uint256 min,
        uint256 max
    ) internal {
        parameters[name] = Parameter({
            name: name,
            value: value,
            minValue: min,
            maxValue: max,
            lastUpdate: block.timestamp,
            proposer: address(0),
            proposalTime: 0
        });
    }

    /**
     * @notice Set parameter (owner only)
     * @param name Parameter name
     * @param value New value
     */
    function setParameter(bytes32 name, uint256 value) external onlyOwner {
        Parameter storage param = parameters[name];
        require(param.maxValue > 0, "Parameter not initialized");

        if (value < param.minValue || value > param.maxValue) {
            revert InvalidValue();
        }

        uint256 oldValue = param.value;
        param.value = value;
        param.lastUpdate = block.timestamp;

        emit ParameterUpdated(name, oldValue, value);
    }

    /**
     * @notice Get parameter value
     * @param name Parameter name
     * @return value Parameter value
     */
    function getParameter(bytes32 name) external view returns (uint256) {
        return parameters[name].value;
    }

    /**
     * @notice Get parameter details
     * @param name Parameter name
     * @return Parameter struct
     */
    function getParameterDetails(bytes32 name)
        external
        view
        returns (Parameter memory)
    {
        return parameters[name];
    }

    // =============================================================================
    // PROPOSALS
    // =============================================================================

    /**
     * @notice Propose parameter change
     * @param name Parameter name
     * @param newValue Proposed new value
     */
    function proposeParameterChange(bytes32 name, uint256 newValue)
        external
        returns (bytes32 proposalId)
    {
        Parameter storage param = parameters[name];
        require(param.maxValue > 0, "Parameter not initialized");

        if (newValue < param.minValue || newValue > param.maxValue) {
            revert InvalidValue();
        }

        // Check voting power
        require(votingPower[msg.sender] >= votingThreshold, "Insufficient voting power");

        // Create proposal
        proposalId = keccak256(
            abi.encodePacked(name, newValue, msg.sender, block.timestamp)
        );

        Proposal storage proposal = proposals[proposalId];
        proposal.paramName = name;
        proposal.newValue = newValue;
        proposal.votes = 0;
        proposal.startTime = block.timestamp;
        proposal.endTime = block.timestamp + votingPeriod;
        proposal.executed = false;

        activeProposals.push(proposalId);

        // Update parameter proposal info
        param.proposer = msg.sender;
        param.proposalTime = block.timestamp;

        emit ParameterProposed(name, newValue, msg.sender);
    }

    /**
     * @notice Vote on proposal
     * @param proposalId Proposal ID
     */
    function vote(bytes32 proposalId) external proposalActive(proposalId) {
        Proposal storage proposal = proposals[proposalId];

        // Check voting power
        uint256 weight = votingPower[msg.sender];
        require(weight > 0, "No voting power");

        // Check not already voted
        require(!proposal.voters[msg.sender], AlreadyVoted());

        // Record vote
        proposal.voters[msg.sender] = true;
        proposal.votes += weight;

        emit VoteCast(proposalId, msg.sender, weight);
    }

    /**
     * @notice Execute proposal
     * @param proposalId Proposal ID
     */
    function executeProposal(bytes32 proposalId) external {
        Proposal storage proposal = proposals[proposalId];

        if (proposal.executed) {
            revert ProposalAlreadyExecuted();
        }

        // Check voting period ended
        if (block.timestamp < proposal.endTime) {
            revert VotingNotActive();
        }

        // Check execution delay passed
        if (block.timestamp < proposal.endTime + executionDelay) {
            revert ExecutionNotReady();
        }

        // Check votes passed
        require(proposal.votes >= votingThreshold, "Proposal did not pass");

        // Update parameter
        Parameter storage param = parameters[proposal.paramName];
        uint256 oldValue = param.value;
        param.value = proposal.newValue;
        param.lastUpdate = block.timestamp;

        // Mark executed
        proposal.executed = true;

        // Remove from active proposals
        _removeActiveProposal(proposalId);

        emit ProposalExecuted(proposalId);
        emit ParameterUpdated(proposal.paramName, oldValue, proposal.newValue);
    }

    /**
     * @notice Remove active proposal
     */
    function _removeActiveProposal(bytes32 proposalId) internal {
        for (uint256 i = 0; i < activeProposals.length; i++) {
            if (activeProposals[i] == proposalId) {
                activeProposals[i] = activeProposals[activeProposals.length - 1];
                activeProposals.pop();
                break;
            }
        }
    }

    // =============================================================================
    // VOTING POWER
    // =============================================================================

    /**
     * @notice Set voting power for account
     * @param account Account address
     * @param power Voting power
     */
    function setVotingPower(address account, uint256 power) external onlyOwner {
        // Remove old power from total
        totalVotingPower -= votingPower[account];

        // Set new power
        votingPower[account] = power;

        // Add new power to total
        totalVotingPower += power;

        emit VotingPowerUpdated(account, power);
    }

    /**
     * @notice Batch set voting power
     * @param accounts Array of accounts
     * @param powers Array of voting powers
     */
    function batchSetVotingPower(address[] calldata accounts, uint256[] calldata powers)
        external
        onlyOwner
    {
        require(accounts.length == powers.length, "Length mismatch");

        for (uint256 i = 0; i < accounts.length; i++) {
            totalVotingPower -= votingPower[accounts[i]];
            votingPower[accounts[i]] = powers[i];
            totalVotingPower += powers[i];
            emit VotingPowerUpdated(accounts[i], powers[i]);
        }
    }

    // =============================================================================
    // GOVERNANCE CONFIG
    // =============================================================================

    /**
     * @notice Set voting period
     * @param period New voting period
     */
    function setVotingPeriod(uint256 period) external onlyOwner {
        require(period > 0, "Invalid period");
        votingPeriod = period;
    }

    /**
     * @notice Set voting threshold
     * @param threshold New voting threshold
     */
    function setVotingThreshold(uint256 threshold) external onlyOwner {
        votingThreshold = threshold;
    }

    /**
     * @notice Set execution delay
     * @param delay New execution delay
     */
    function setExecutionDelay(uint256 delay) external onlyOwner {
        require(delay > 0, "Invalid delay");
        executionDelay = delay;
    }

    // =============================================================================
    // QUERY FUNCTIONS
    // =============================================================================

    /**
     * @notice Get active proposals count
     * @return count Number of active proposals
     */
    function getActiveProposalsCount() external view returns (uint256) {
        return activeProposals.length;
    }

    /**
     * @notice Get active proposal IDs
     * @return Array of proposal IDs
     */
    function getActiveProposals() external view returns (bytes32[] memory) {
        return activeProposals;
    }

    /**
     * @notice Check if account has voted on proposal
     * @param proposalId Proposal ID
     * @param account Account address
     * @return bool True if voted
     */
    function hasVoted(bytes32 proposalId, address account)
        external
        view
        returns (bool)
    {
        return proposals[proposalId].voters[account];
    }

    /**
     * @notice Get proposal votes
     * @param proposalId Proposal ID
     * @return votes Current vote count
     */
    function getProposalVotes(bytes32 proposalId) external view returns (uint256) {
        return proposals[proposalId].votes;
    }

    // =============================================================================
    // INIT
    // =============================================================================

    // Avoid unused warnings
    function init() external pure {
        require(votingPeriod > 0, "period check");
    }
}