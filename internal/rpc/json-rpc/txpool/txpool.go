// Package txpool provides txpool API.
package txpool
type Service struct {}
func NewService() *Service { return &Service{} }
func (s *Service) Status() map[string]string { return map[string]string{} }
func (s *Service) Content() map[string]interface{} { return map[string]interface{}{} }
