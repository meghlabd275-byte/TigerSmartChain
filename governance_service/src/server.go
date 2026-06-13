// Package governance provides governance service for proposals and voting
package governance

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"
)

type Config struct {
	DBURL          string
	RedisURL       string
	UpdateInterval time.Duration
}

type Proposal struct {
	ID            int       `json:"id"`
	Proposer     string    `json:"proposer"`
	Title        string    `json:"title"`
	Description  string    `json:"description"`
	TargetContract string   `json:"targetContract"`
	CallData     string    `json:"callData"`
	Value        string    `json:"value"`
	Status       string    `json:"status"`
	ForVotes    float64   `json:"forVotes"`
	AgainstVotes float64   `json:"againstVotes"`
	AbstainVotes float64   `json:"abstainVotes"`
	StartBlock   int64     `json:"startBlock"`
	EndBlock    int64     `json:"endBlock"`
	ETA         *int64    `json:"eta"`
	CreatedAt   time.Time `json:"createdAt"`
}

type Vote struct {
	ProposalID int       `json:"proposalId"`
	Voter      string    `json:"voter"`
	Support    bool      `json:"support"`
	Votes      float64   `json:"votes"`
	Reason     string    `json:"reason"`
	Timestamp  time.Time `json:"timestamp"`
}

type Server struct {
	cfg   *Config
	pool  *pgxpool.Pool
	redis *redis.Client
	mu    sync.RWMutex
}

func NewServer(cfg *Config) (*Server, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, cfg.DBURL)
	if err != nil {
		return nil, err
	}
	rdb := redis.NewClient(&redis.Options{Addr: cfg.RedisURL, Password: "", DB: 13})
	if err := rdb.Ping(ctx).Err(); err != nil {
		return nil, err
	}
	createTables(ctx, pool)
	return &Server{cfg: cfg, pool: pool, redis: rdb}, nil
}

func createTables(ctx context.Context, pool *pgxpool.Pool) {
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS proposals (id SERIAL PRIMARY KEY, proposer VARCHAR(42) NOT NULL, title VARCHAR(255) NOT NULL, description TEXT, target_contract VARCHAR(42), call_data TEXT, value VARCHAR(66), status VARCHAR(20) DEFAULT 'pending', for_votes VARCHAR(66) DEFAULT '0', against_votes VARCHAR(66) DEFAULT '0', abstain_votes VARCHAR(66) DEFAULT '0', start_block BIGINT, end_block BIGINT, eta BIGINT, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)`)
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS votes (id SERIAL PRIMARY KEY, proposal_id INTEGER NOT NULL, voter VARCHAR(42) NOT NULL, support BOOLEAN NOT NULL, votes VARCHAR(66) NOT NULL, reason TEXT, timestamp BIGINT NOT NULL, UNIQUE(proposal_id, voter))`)
	pool.Exec(ctx, `CREATE TABLE IF NOT EXISTS delegates (id SERIAL PRIMARY KEY, delegatee VARCHAR(42) NOT NULL, delegator VARCHAR(42) NOT NULL, votes VARCHAR(66) NOT NULL, timestamp BIGINT NOT NULL, UNIQUE(delegatee, delegator))`)
}

func (s *Server) GetProposals(ctx context.Context, status string) ([]Proposal, error) {
	query := "SELECT id, proposer, title, description, status, for_votes, against_votes, abstain_votes, start_block, end_block, created_at FROM proposals"
	if status != "" {
		query += " WHERE status = '" + status + "'"
	}
	query += " ORDER BY id DESC"
	
	rows, err := s.pool.Query(ctx, query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var proposals []Proposal
	for rows.Next() {
		var p Proposal
		var forVotes, againstVotes, abstainVotes string
		if err := rows.Scan(&p.ID, &p.Proposer, &p.Title, &p.Description, &p.Status, &forVotes, &againstVotes, &abstainVotes, &p.StartBlock, &p.EndBlock, &p.CreatedAt); err != nil {
			continue
		}
		fmt.Sscanf(forVotes, "%f", &p.ForVotes)
		fmt.Sscanf(againstVotes, "%f", &p.AgainstVotes)
		fmt.Sscanf(abstainVotes, "%f", &p.AbstainVotes)
		proposals = append(proposals, p)
	}
	return proposals, nil
}

func (s *Server) GetProposal(ctx context.Context, proposalID int) (*Proposal, error) {
	var p Proposal
	var forVotes, againstVotes, abstainVotes string
	err := s.pool.QueryRow(ctx, "SELECT id, proposer, title, description, status, for_votes, against_votes, abstain_votes, start_block, end_block, created_at FROM proposals WHERE id = $1", proposalID).Scan(&p.ID, &p.Proposer, &p.Title, &p.Description, &p.Status, &forVotes, &againstVotes, &abstainVotes, &p.StartBlock, &p.EndBlock, &p.CreatedAt)
	if err != nil {
		return nil, err
	}
	fmt.Sscanf(forVotes, "%f", &p.ForVotes)
	fmt.Sscanf(againstVotes, "%f", &p.AgainstVotes)
	fmt.Sscanf(abstainVotes, "%f", &p.AbstainVotes)
	return &p, nil
}

func (s *Server) CastVote(ctx context.Context, proposalID int, voter string, support bool, votes float64, reason string) error {
	votesStr := fmt.Sprintf("%.0f", votes)
	supportStr := "false"
	if support {
		supportStr = "true"
	}
	_, err := s.pool.Exec(ctx, "INSERT INTO votes (proposal_id, voter, support, votes, reason, timestamp) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (proposal_id, voter) DO UPDATE SET support = $3, votes = $4, reason = $5, timestamp = $6", proposalID, voter, supportStr, votesStr, reason, time.Now().Unix())
	if err != nil {
		return err
	}
	if support {
		s.pool.Exec(ctx, "UPDATE proposals SET for_votes = for_votes + $1 WHERE id = $2", votesStr, proposalID)
	} else {
		s.pool.Exec(ctx, "UPDATE proposals SET against_votes = against_votes + $1 WHERE id = $2", votesStr, proposalID)
	}
	return nil
}

func (s *Server) GetVotes(ctx context.Context, proposalID int) ([]Vote, error) {
	rows, err := s.pool.Query(ctx, "SELECT proposal_id, voter, support, votes, reason, timestamp FROM votes WHERE proposal_id = $1 ORDER BY votes DESC", proposalID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var votes []Vote
	for rows.Next() {
		var v Vote
		var supportStr, votesStr string
		var timestamp int64
		if err := rows.Scan(&v.ProposalID, &v.Voter, &supportStr, &votesStr, &v.Reason, &timestamp); err != nil {
			continue
		}
		v.Support = supportStr == "true"
		fmt.Sscanf(votesStr, "%f", &v.Votes)
		v.Timestamp = time.Unix(timestamp, 0)
		votes = append(votes, v)
	}
	return votes, nil
}

func (s *Server) GetVoterVotes(ctx context.Context, voter string) ([]Vote, error) {
	rows, err := s.pool.Query(ctx, "SELECT proposal_id, voter, support, votes, reason, timestamp FROM votes WHERE voter = $1 ORDER BY timestamp DESC", voter)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	
	var votes []Vote
	for rows.Next() {
		var v Vote
		var supportStr, votesStr string
		var timestamp int64
		if err := rows.Scan(&v.ProposalID, &v.Voter, &supportStr, &votesStr, &v.Reason, &timestamp); err != nil {
			continue
		}
		v.Support = supportStr == "true"
		fmt.Sscanf(votesStr, "%f", &v.Votes)
		v.Timestamp = time.Unix(timestamp, 0)
		votes = append(votes, v)
	}
	return votes, nil
}

func CalculateQuorum(totalSupply, votes float64) float64 {
	if totalSupply == 0 {
		return 0
	}
	return (votes / totalSupply) * 100
}

func FormatVotes(votes float64) string {
	if votes >= 1e6 {
		return fmt.Sprintf("%.2fM", votes/1e6)
	}
	if votes >= 1e3 {
		return fmt.Sprintf("%.2fK", votes/1e3)
	}
	return fmt.Sprintf("%.0f", votes)
}