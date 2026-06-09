// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Governance Contract
/// @notice On-chain governance for TigerSmartChain

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/// @title Governor
/// @dev Governance contract with proposal, voting, and execution
contract Governor is AccessControl, ReentrancyGuard {
    using SafeERC20 for IERC20;
    using ECDSA for bytes32;

    /// @notice Voting delay (blocks)
    uint256 public votingDelay;

    /// @notice Voting period (blocks)
    uint256 public votingPeriod;

    /// @notice Proposal threshold (votes required)
    uint256 public proposalThreshold;

    /// @notice Quorum required (votes)
    uint256 public quorumVotes;

    /// @notice Timelock delay (seconds)
    uint256 public timelockDelay;

    /// @notice Proposal states
    enum ProposalState {
        Pending,
        Active,
        Canceled,
        Defeated,
        Succeeded,
        Queued,
        Executed,
        Expired
    }

    /// @notice Proposal
    struct Proposal {
        uint256 id;
        address proposer;
        address[] targets;
        uint256[] values;
        string[] signatures;
        bytes[] calldatas;
        uint256 startBlock;
        uint256 endBlock;
        uint256 forVotes;
        uint256 againstVotes;
        uint256 abstainVotes;
        bool canceled;
        bool executed;
        bool queued;
        uint256 queueTime;
        bytes32 descriptionHash;
    }

    /// @notice Proposal votes
    struct ProposalVote {
        bool hasVoted;
        bool support;
        uint256 votes;
    }

    /// @notice Token used for voting
    IERC20 public immutable token;

    /// @notice Timelock contract
    address public timelock;

    /// @notice Proposal count
    uint256 public proposalCount;

    /// @notice Proposals mapping
    mapping(uint256 => Proposal) public proposals;

    /// @notice Receipts mapping (proposalId => voter => Receipt)
    mapping(uint256 => mapping(address => ProposalVote)) public proposalReceipts;

    /// @notice Proposal created event
    event ProposalCreated(
        uint256 id,
        address proposer,
        address[] targets,
        uint256[] values,
        string[] signatures,
        bytes[] calldatas,
        uint256 startBlock,
        uint256 endBlock,
        string description
    );

    /// @notice Vote cast event
    event VoteCast(
        address voter,
        uint256 proposalId,
        bool support,
        uint256 votes
    );

    /// @notice Proposal executed event
    event ProposalExecuted(uint256 id);

    /// @notice Proposal queued event
    event ProposalQueued(uint256 id, uint256 eta);

    /// @notice Proposal canceled event
    event ProposalCanceled(uint256 id);

    /// @notice Constructor
    /// @param _token Voting token
    /// @param _timelock Timelock contract
    /// @param _votingDelay Voting delay in blocks
    /// @param _votingPeriod Voting period in blocks
    /// @param _proposalThreshold Minimum votes to propose
    /// @param _quorumVotes Quorum required
    constructor(
        address _token,
        address _timelock,
        uint256 _votingDelay,
        uint256 _votingPeriod,
        uint256 _proposalThreshold,
        uint256 _quorumVotes
    ) {
        require(_token != address(0), "Invalid token");
        require(_timelock != address(0), "Invalid timelock");

        token = IERC20(_token);
        timelock = _timelock;
        votingDelay = _votingDelay;
        votingPeriod = _votingPeriod;
        proposalThreshold = _proposalThreshold;
        quorumVotes = _quorumVotes;

        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
    }

    /// @notice Create a proposal
    function propose(
        address[] memory targets,
        uint256[] memory values,
        string[] memory signatures,
        bytes[] memory calldatas,
        string memory description
    ) public returns (uint256) {
        // Check proposal threshold
        require(
            token.balanceOf(msg.sender) >= proposalThreshold,
            "Votes below threshold"
        );
        require(
            targets.length == values.length &&
                targets.length == signatures.length &&
                targets.length == calldatas.length,
            "Length mismatch"
        );
        require(targets.length > 0, "No actions");

        uint256 startBlock = block.number + votingDelay;
        uint256 endBlock = startBlock + votingPeriod;

        proposalCount++;
        uint256 proposalId = proposalCount;

        Proposal storage proposal = proposals[proposalId];
        proposal.id = proposalId;
        proposal.proposer = msg.sender;
        proposal.targets = targets;
        proposal.values = values;
        proposal.signatures = signatures;
        proposal.calldatas = calldatas;
        proposal.startBlock = startBlock;
        proposal.endBlock = endBlock;
        proposal.descriptionHash = keccak256(bytes(description));

        emit ProposalCreated(
            proposalId,
            msg.sender,
            targets,
            values,
            signatures,
            calldatas,
            startBlock,
            endBlock,
            description
        );

        return proposalId;
    }

    /// @notice Cast vote
    function castVote(uint256 proposalId, bool support) public returns (uint256) {
        return _castVote(msg.sender, proposalId, support);
    }

    /// @notice Cast vote with reason
    function castVoteWithReason(
        uint256 proposalId,
        bool support,
        string memory reason
    ) public returns (uint256) {
        uint256 votes = _castVote(msg.sender, proposalId, support);
        return votes;
    }

    /// @notice Cast vote by signature
    function castVoteBySig(
        uint256 proposalId,
        bool support,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) public returns (uint256) {
        bytes32 domainSeparator = keccak256(
            abi.encode(
                keccak256(
                    "EIP712Domain(string name,uint256 chainId,address verifyingContract)"
                ),
                keccak256("Governor"),
                block.chainid,
                address(this)
            )
        );

        bytes32 hash = keccak256(
            abi.encodePacked(
                "\x19\x01",
                domainSeparator,
                keccak256(
                    abi.encode(proposalId, support)
                )
            )
        );

        address signer = ecrecover(hash, v, r, s);
        require(signer != address(0), "Invalid signature");

        return _castVote(signer, proposalId, support);
    }

    /// @dev Internal vote casting
    function _castVote(
        address voter,
        uint256 proposalId,
        bool support
    ) internal returns (uint256) {
        Proposal storage proposal = proposals[proposalId];
        require(state(proposalId) == ProposalState.Active, "Not active");

        ProposalVote storage receipt = proposalReceipts[proposalId][voter];
        require(!receipt.hasVoted, "Already voted");

        uint256 votes = token.balanceOf(voter);
        require(votes > 0, "No votes");

        receipt.hasVoted = true;
        receipt.support = support;
        receipt.votes = votes;

        if (support) {
            proposal.forVotes += votes;
        } else {
            proposal.againstVotes += votes;
        }

        emit VoteCast(voter, proposalId, support, votes);

        return votes;
    }

    /// @notice Execute proposal
    function execute(uint256 proposalId)
        public
        payable
        nonReentrant
        returns (uint256)
    {
        Proposal storage proposal = proposals[proposalId];
        require(
            state(proposalId) == ProposalState.Succeeded ||
                state(proposalId) == ProposalState.Queued,
            "Not executable"
        );
        require(!proposal.executed, "Already executed");

        proposal.executed = true;

        for (uint256 i = 0; i < proposal.targets.length; i++) {
            (bool success, ) = proposal.targets[i].call{value: proposal.values[i]}(
                proposal.calldatas[i]
            );
            require(success, "Call failed");
        }

        emit ProposalExecuted(proposalId);

        return proposalId;
    }

    /// @notice Queue proposal for execution
    function queue(uint256 proposalId) public {
        Proposal storage proposal = proposals[proposalId];
        require(state(proposalId) == ProposalState.Succeeded, "Not succeeded");

        uint256 eta = block.timestamp + timelockDelay;
        proposal.queued = true;
        proposal.queueTime = eta;

        emit ProposalQueued(proposalId, eta);
    }

    /// @notice Cancel proposal
    function cancel(uint256 proposalId) public {
        Proposal storage proposal = proposals[proposalId];
        require(
            msg.sender == proposal.proposer || hasRole(DEFAULT_ADMIN_ROLE, msg.sender),
            "Not authorized"
        );
        require(
            state(proposalId) != ProposalState.Executed,
            "Already executed"
        );

        proposal.canceled = true;

        emit ProposalCanceled(proposalId);
    }

    /// @notice Get proposal state
    function state(uint256 proposalId) public view returns (ProposalState) {
        Proposal storage proposal = proposals[proposalId];

        if (proposal.canceled) {
            return ProposalState.Canceled;
        } else if (block.number <= proposal.startBlock) {
            return ProposalState.Pending;
        } else if (block.number <= proposal.endBlock) {
            return ProposalState.Active;
        } else if (
            proposal.forVotes <= proposal.againstVotes ||
            proposal.forVotes < quorumVotes
        ) {
            return ProposalState.Defeated;
        } else if (proposal.queued && block.timestamp < proposal.queueTime) {
            return ProposalState.Queued;
        } else if (proposal.queued && block.timestamp >= proposal.queueTime) {
            return ProposalState.Succeeded;
        } else if (proposal.executed) {
            return ProposalState.Executed;
        } else {
            return ProposalState.Succeeded;
        }
    }

    /// @notice Set voting delay
    function setVotingDelay(uint256 _votingDelay)
        external
        onlyRole(DEFAULT_ADMIN_ROLE)
    {
        votingDelay = _votingDelay;
    }

    /// @notice Set voting period
    function setVotingPeriod(uint256 _votingPeriod)
        external
        onlyRole(DEFAULT_ADMIN_ROLE)
    {
        votingPeriod = _votingPeriod;
    }

    /// @notice Set proposal threshold
    function setProposalThreshold(uint256 _proposalThreshold)
        external
        onlyRole(DEFAULT_ADMIN_ROLE)
    {
        proposalThreshold = _proposalThreshold;
    }

    /// @notice Set quorum votes
    function setQuorumVotes(uint256 _quorumVotes)
        external
        onlyRole(DEFAULT_ADMIN_ROLE)
    {
        quorumVotes = _quorumVotes;
    }

    /// @notice Set timelock delay
    function setTimelockDelay(uint256 _timelockDelay)
        external
        onlyRole(DEFAULT_ADMIN_ROLE)
    {
        timelockDelay = _timelockDelay;
    }
}

/// @title Timelock
/// @dev Executable after delay
contract Timelock is AccessControl {
    using SafeERC20 for IERC20;

    /// @notice Minimum delay
    uint256 public minDelay;

    /// @notice Pending calls
    mapping(bytes32 => bool) public queuedTransactions;

    /// @notice Call scheduled event
    event CallScheduled(
        bytes32 indexed txHash,
        address indexed target,
        uint256 value,
        bytes data,
        uint256 eta
    );

    /// @notice Call executed event
    event CallExecuted(
        bytes32 indexed txHash,
        address indexed target,
        uint256 value,
        bytes data
    );

    /// @notice Call canceled event
    event CallCanceled(bytes32 indexed txHash);

    /// @notice Constructor
    /// @param _minDelay Minimum delay
    constructor(uint256 _minDelay) {
        minDelay = _minDelay;
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
    }

    /// @notice Schedule a call
    function schedule(
        address target,
        uint256 value,
        bytes calldata data,
        uint256 eta
    ) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(eta >= block.timestamp + minDelay, "Delay not met");

        bytes32 txHash = keccak256(
            abi.encode(target, value, data, eta)
        );
        queuedTransactions[txHash] = true;

        emit CallScheduled(txHash, target, value, data, eta);
    }

    /// @notice Execute a call
    function execute(
        address target,
        uint256 value,
        bytes calldata data,
        uint256 eta
    ) external payable onlyRole(DEFAULT_ADMIN_ROLE) {
        bytes32 txHash = keccak256(
            abi.encode(target, value, data, eta)
        );
        require(queuedTransactions[txHash], "Not queued");

        queuedTransactions[txHash] = false;

        (bool success, ) = target.call{value: value}(data);
        require(success, "Call failed");

        emit CallExecuted(txHash, target, value, data);
    }

    /// @notice Cancel a call
    function cancel(bytes32 txHash) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(queuedTransactions[txHash], "Not queued");
        queuedTransactions[txHash] = false;

        emit CallCanceled(txHash);
    }
}