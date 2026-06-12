// Package dex provides DEX integration.
package dex
type Service struct {}
func NewService(rpcURL, graphQLURL string) *Service { return &Service{} }
func (s *Service) GetTopPairs(limit int) ([]*PairInfo, error) { return nil, nil }
type PairInfo struct{}
