// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

/**
 * @title ChallengeSystem
 * @dev Challenge system for cross-chain bridge security.
 * Implements fraud proof verification and challenge period.
 */
contract ChallengeSystem {
    // Events
    event ChallengeCreated(bytes32 indexed challengeId, address challenger, uint256 bond);
    event ChallengeResolved(bytes32 indexed challengeId, bool success);
    event ChallengeExpired(bytes32 indexed challengeId);

    // Constants
    uint256 public constant CHALLENGE_PERIOD = 24 hours;
    uint256 public constant CHALLENGE_BOND = 1000 ether;
    uint256 public constant MAX_CHALLENGES = 100;

    // State
    mapping(bytes32 => Challenge) public challenges;
    mapping(address => uint256) public challengerBonds;
    address[] public activeChallengers;
    uint256 public challengeCount;

    struct Challenge {
        address challenger;
        bytes32 messageHash;
        uint256 blockNumber;
        uint256 timestamp;
        bool resolved;
        bool success;
        string evidence;
    }

    modifier onlyActiveChallenge(bytes32 challengeId) {
        require(challenges[challengeId].challenger != address(0), "Challenge not exist");
        require(!challenges[challengeId].resolved, "Challenge resolved");
        require(block.timestamp <= challenges[challengeId].timestamp + CHALLENGE_PERIOD, "Challenge expired");
        _;
    }

    /**
     * @dev Create a challenge for a cross-chain message
     * @param messageHash Hash of the message to challenge
     * @param evidence Evidence supporting the challenge
     */
    function createChallenge(bytes32 messageHash, string calldata evidence) external payable {
        require(msg.value >= CHALLENGE_BOND, "Insufficient bond");
        require(challengeCount < MAX_CHALLENGES, "Max challenges reached");

        bytes32 challengeId = keccak256(abi.encodePacked(messageHash, msg.sender, block.timestamp));
        
        challenges[challengeId] = Challenge({
            challenger: msg.sender,
            messageHash: messageHash,
            blockNumber: block.number,
            timestamp: block.timestamp,
            resolved: false,
            success: false,
            evidence: evidence
        });

        challengerBonds[msg.sender] += msg.value;
        activeChallengers.push(msg.sender);
        challengeCount++;

        emit ChallengeCreated(challengeId, msg.sender, msg.value);
    }

    /**
     * @dev Resolve a challenge
     * @param challengeId ID of the challenge
     * @param success Whether the challenge was valid
     */
    function resolveChallenge(bytes32 challengeId, bool success) 
        external 
        onlyActiveChallenge(challengeId) 
    {
        Challenge storage challenge = challenges[challengeId];
        challenge.resolved = true;
        challenge.success = success;

        if (success) {
            // Return bond to challenger
            payable(challenge.challenger).transfer(challengerBonds[challenge.challenger]);
            challengerBonds[challenge.challenger] = 0;
        }

        emit ChallengeResolved(challengeId, success);
    }

    /**
     * @dev Expire a challenge that passed the challenge period
     * @param challengeId ID of the challenge
     */
    function expireChallenge(bytes32 challengeId) external {
        Challenge storage challenge = challenges[challengeId];
        require(challenge.challenger != address(0), "Challenge not exist");
        require(!challenge.resolved, "Already resolved");
        require(block.timestamp > challenge.timestamp + CHALLENGE_PERIOD, "Challenge period not ended");

        challenge.resolved = true;
        challenge.success = false;

        emit ChallengeExpired(challengeId);
    }

    /**
     * @dev Get challenge details
     * @param challengeId ID of the challenge
     */
    function getChallenge(bytes32 challengeId) external view returns (
        address challenger,
        bytes32 messageHash,
        uint256 blockNumber,
        uint256 timestamp,
        bool resolved,
        bool success
    ) {
        Challenge storage c = challenges[challengeId];
        return (
            c.challenger,
            c.messageHash,
            c.blockNumber,
            c.timestamp,
            c.resolved,
            c.success
        );
    }

    /**
     * @dev Get active challenge count
     */
    function getActiveChallengeCount() external view returns (uint256) {
        uint256 count = 0;
        for (uint256 i = 0; i < activeChallengers.length; i++) {
            if (challengerBonds[activeChallengers[i]] > 0) {
                count++;
            }
        }
        return count;
    }

    // Receive ETH
    receive() external payable {}
}