// Package ens provides ENS resolution.
package ens

type Service struct {}
func NewService(rpcURL string) (*Service, error) { return &Service{}, nil }
func (s *Service) Resolve(name string) (string, error) { return "", nil }
func (s *Service) ReverseResolve(addr string) (string, error) { return "", nil }
