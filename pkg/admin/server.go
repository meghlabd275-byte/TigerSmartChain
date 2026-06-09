// Package admin provides HTTP server for white-label admin system.
package admin

import (
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"strings"
	"time"
)

// HTTPServer provides HTTP endpoints for admin system.
type HTTPServer struct {
	admin *WhiteLabelAdmin
	mux   *http.ServeMux
}

// NewHTTPServer creates new HTTP server.
func NewHTTPServer(admin *WhiteLabelAdmin) *HTTPServer {
	hs := &HTTPServer{
		admin: admin,
		mux:   http.NewServeMux(),
	}
	
	hs.registerRoutes()
	return hs
}

func (hs *HTTPServer) registerRoutes() {
	// CORS wrapper for all routes
	handler := hs.corsMiddleware(hs.router)
	hs.mux.Handle("/", handler)
}

func (hs *HTTPServer) corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Set CORS headers
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization, X-API-Key, X-CSRF-Token")
		w.Header().Set("Access-Control-Max-Age", "3600")
		
		// Handle preflight
		if r.Method == "OPTIONS" {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		
		// Security headers
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("X-Frame-Options", "DENY")
		w.Header().Set("X-XSS-Protection", "1; mode=block")
		w.Header().Set("Strict-Transport-Security", "max-age=31536000; includeSubDomains")
		
		next.ServeHTTP(w, r)
	})
}

func (hs *HTTPServer) router(w http.ResponseWriter, r *http.Request) {
	path := r.URL.Path
	method := r.Method
	
	// Rate limiting
	clientIP := getClientIP(r)
	if !hs.admin.CheckRateLimit(clientIP) {
		hs.writeError(w, http.StatusTooManyRequests, "rate limit exceeded")
		return
	}
	
	switch {
	// Health check
	case path == "/health":
		hs.handleHealth(w, r)
		
	// Auth routes
	case path == "/api/v1/register" && method == "POST":
		hs.handleRegister(w, r)
	case path == "/api/v1/login" && method == "POST":
		hs.handleLogin(w, r)
	case path == "/api/v1/logout" && method == "POST":
		hs.handleLogout(w, r)
	case path == "/api/v1/verify" && method == "POST":
		hs.handleVerify(w, r)
		
	// User routes (require auth)
	case path == "/api/v1/users" && method == "GET":
		hs.handleListUsers(w, r)
	case strings.HasPrefix(path, "/api/v1/users/") && method == "GET":
		hs.handleGetUser(w, r)
		
	// Admin approval
	case path == "/api/v1/approve" && method == "POST":
		hs.handleApproveUser(w, r)
	case path == "/api/v1/approve-admin" && method == "POST":
		hs.handleApproveAsAdmin(w, r)
		
	// Product routes
	case path == "/api/v1/products" && method == "POST":
		hs.handleCreateProduct(w, r)
	case path == "/api/v1/products" && method == "GET":
		hs.handleListProducts(w, r)
	case strings.HasPrefix(path, "/api/v1/products/") && method == "GET":
		hs.handleGetProduct(w, r)
	case path == "/api/v1/products/approve" && method == "POST":
		hs.handleApproveProduct(w, r)
	case path == "/api/v1/products/pause" && method == "POST":
		hs.handlePauseProduct(w, r)
	case path == "/api/v1/products/resume" && method == "POST":
		hs.handleResumeProduct(w, r)
	case path == "/api/v1/products/halt" && method == "POST":
		hs.handleHaltProduct(w, r)
	case path == "/api/v1/products/destroy" && method == "POST":
		hs.handleDestroyProduct(w, r)
		
	// API key routes
	case path == "/api/v1/apikeys" && method == "POST":
		hs.handleCreateAPIKey(w, r)
	case path == "/api/v1/apikeys" && method == "GET":
		hs.handleListAPIKeys(w, r)
	case path == "/api/v1/apikeys/revoke" && method == "POST":
		hs.handleRevokeAPIKey(w, r)
		
	// Admin routes
	case path == "/api/v1/admins" && method == "POST":
		hs.handleCreateAdmin(w, r)
	case path == "/api/v1/admins" && method == "GET":
		hs.handleListAdmins(w, r)
	case path == "/api/v1/admins/remove" && method == "POST":
		hs.handleRemoveAdmin(w, r)
		
	// Permission routes
	case path == "/api/v1/permissions/grant" && method == "POST":
		hs.handleGrantPermission(w, r)
	case path == "/api/v1/permissions/revoke" && method == "POST":
		hs.handleRevokePermission(w, r)
		
	// Audit logs
	case path == "/api/v1/audit" && method == "GET":
		hs.handleGetAuditLogs(w, r)
		
	// Stats
	case path == "/api/v1/stats" && method == "GET":
		hs.handleGetStats(w, r)
		
	default:
		hs.writeError(w, http.StatusNotFound, "endpoint not found")
	}
}

// Request/Response types
type RegisterRequest struct {
	Username string `json:"username"`
	Email    string `json:"email"`
	Password string `json:"password"`
}

type LoginRequest struct {
	Email    string `json:"email"`
	Password string `json:"password"`
}

type ApproveRequest struct {
	UserID  string `json:"user_id"`
	AdminID string `json:"admin_id"`
}

type ProductRequest struct {
	OwnerID   string `json:"owner_id"`
	Name     string `json:"name"`
	BrandName string `json:"brand_name"`
	Domain   string `json:"domain"`
	Cloud    string `json:"cloud"`
	Storage  string `json:"storage"`
	AdminID  string `json:"admin_id"`
}

type APIKeyRequest struct {
	ProductID string `json:"product_id"`
	AdminID  string `json:"admin_id"`
	Name     string `json:"name"`
	ExpiresAt uint64 `json:"expires_at"`
}

type AdminRequest struct {
	SuperAdminID string `json:"super_admin_id"`
	Username   string `json:"username"`
	Email     string `json:"email"`
	Password  string `json:"password"`
}

type PermissionRequest struct {
	TargetID    string `json:"target_id"`
	AdminID    string `json:"admin_id"`
	Permission string `json:"permission"`
}

// Handlers
func (hs *HTTPServer) handleHealth(w http.ResponseWriter, r *http.Request) {
	hs.writeJSON(w, map[string]string{"status": "healthy"})
}

func (hs *HTTPServer) handleRegister(w http.ResponseWriter, r *http.Request) {
	var req RegisterRequest
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	// Input sanitization
	req.Username = SanitizeInput(req.Username)
	req.Email = strings.ToLower(strings.TrimSpace(req.Email))
	
	// Validate password strength
	if err := validatePassword(req.Password); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	userID, err := hs.admin.Register(req.Username, req.Email, req.Password, getClientIP(r))
	if err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{
		"user_id":  userID,
		"message": "Registration successful. Please wait for admin approval.",
	})
}

func (hs *HTTPServer) handleLogin(w http.ResponseWriter, r *http.Request) {
	var req LoginRequest
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	// Check rate limit
	if !hs.admin.CheckRateLimit(getClientIP(r) + ":login") {
		hs.writeError(w, http.StatusTooManyRequests, "Too many login attempts. Please try again later.")
		return
	}
	
	session, err := hs.admin.Login(req.Email, req.Password, getClientIP(r), r.UserAgent())
	if err != nil {
		errMsg := err.Error()
		if errMsg == "2fa_required" {
			hs.writeJSON(w, map[string]string{
				"requires_2fa": "true",
				"message":      "Please enter your 2FA code",
			})
			return
		}
		if errMsg == "account pending approval. Please contact admin for authorization" {
			hs.writeError(w, http.StatusForbidden, errMsg)
			return
		}
		hs.writeError(w, http.StatusUnauthorized, "Invalid credentials")
		return
	}
	
	hs.writeJSON(w, map[string]string{
		"session_id": session,
		"message":  "Login successful",
	})
}

func (hs *HTTPServer) handleLogout(w http.ResponseWriter, r *http.Request) {
	sessionID := r.Header.Get("Authorization")
	if sessionID == "" {
		hs.writeError(w, http.StatusUnauthorized, "Authorization required")
		return
	}
	
	if err := hs.admin.Logout(sessionID); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "Logout successful"})
}

func (hs *HTTPServer) handleVerify(w http.ResponseWriter, r *http.Request) {
	sessionID := r.Header.Get("Authorization")
	if sessionID == "" {
		hs.writeError(w, http.StatusUnauthorized, "Authorization required")
		return
	}
	
	userID, err := hs.admin.VerifySession(sessionID)
	if err != nil {
		hs.writeError(w, http.StatusUnauthorized, "Invalid session")
		return
	}
	
	user, exists := hs.admin.GetUser(userID)
	if !exists {
		hs.writeError(w, http.StatusNotFound, "User not found")
		return
	}
	
	hs.writeJSON(w, map[string]interface{}{
		"user_id":  user.ID,
		"username": user.Username,
		"email":    user.Email,
		"role":     user.Role.String(),
		"status":   user.Status.String(),
	})
}

func (hs *HTTPServer) handleListUsers(w http.ResponseWriter, r *http.Request) {
	// Require admin auth
	adminID, err := hs.requireAdmin(w, r)
	if err != nil {
		return
	}
	
	users := hs.admin.GetUsers()
	hs.writeJSON(w, users)
}

func (hs *HTTPServer) handleGetUser(w http.ResponseWriter, r *http.Request) {
	// Require admin auth
	if _, err := hs.requireAdmin(w, r); err != nil {
		return
	}
	
	userID := strings.TrimPrefix(r.URL.Path, "/api/v1/users/")
	user, exists := hs.admin.GetUser(userID)
	if !exists {
		hs.writeError(w, http.StatusNotFound, "User not found")
		return
	}
	
	hs.writeJSON(w, user)
}

func (hs *HTTPServer) handleApproveUser(w http.ResponseWriter, r *http.Request) {
	var req ApproveRequest
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.ApproveUser(req.UserID, req.AdminID, getClientIP(r)); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "User approved successfully"})
}

func (hs *HTTPServer) handleApproveAsAdmin(w http.ResponseWriter, r *http.Request) {
	var req ApproveRequest
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.ApproveUserAsAdmin(req.UserID, req.AdminID, getClientIP(r)); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "User approved as admin successfully"})
}

func (hs *HTTPServer) handleCreateProduct(w http.ResponseWriter, r *http.Request) {
	var req ProductRequest
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	productID, err := hs.admin.CreateProduct(
		req.OwnerID,
		req.Name,
		req.BrandName,
		req.Domain,
		req.Cloud,
		req.Storage,
		req.AdminID,
	)
	if err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{
		"product_id": productID,
		"message":   "Product created. Please wait for admin approval.",
	})
}

func (hs *HTTPServer) handleListProducts(w http.ResponseWriter, r *http.Request) {
	// Require auth
	if _, err := hs.requireAdmin(w, r); err != nil {
		return
	}
	
	products := hs.admin.GetProducts()
	hs.writeJSON(w, products)
}

func (hs *HTTPServer) handleGetProduct(w http.ResponseWriter, r *http.Request) {
	productID := strings.TrimPrefix(r.URL.Path, "/api/v1/products/")
	product, exists := hs.admin.GetProduct(productID)
	if !exists {
		hs.writeError(w, http.StatusNotFound, "Product not found")
		return
	}
	
	hs.writeJSON(w, product)
}

func (hs *HTTPServer) handleApproveProduct(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ProductID string `json:"product_id"`
		AdminID  string `json:"admin_id"`
	}
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.ApproveProduct(req.ProductID, req.AdminID, getClientIP(r)); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "Product approved successfully"})
}

func (hs *HTTPServer) handlePauseProduct(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ProductID string `json:"product_id"`
		AdminID  string `json:"admin_id"`
	}
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.PauseProduct(req.ProductID, req.AdminID, getClientIP(r)); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "Product paused successfully"})
}

func (hs *HTTPServer) handleResumeProduct(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ProductID string `json:"product_id"`
		AdminID  string `json:"admin_id"`
	}
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.ResumeProduct(req.ProductID, req.AdminID, getClientIP(r)); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "Product resumed successfully"})
}

func (hs *HTTPServer) handleHaltProduct(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ProductID string `json:"product_id"`
		AdminID  string `json:"admin_id"`
	}
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.HaltProduct(req.ProductID, req.AdminID, getClientIP(r)); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "Product halted successfully"})
}

func (hs *HTTPServer) handleDestroyProduct(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ProductID string `json:"product_id"`
		AdminID  string `json:"admin_id"`
	}
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.DestroyProduct(req.ProductID, req.AdminID, getClientIP(r)); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "Product destroyed successfully"})
}

func (hs *HTTPServer) handleCreateAPIKey(w http.ResponseWriter, r *http.Request) {
	var req APIKeyRequest
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	apiKey, err := hs.admin.GenerateProductAPIKey(req.ProductID, req.AdminID, req.Name, req.ExpiresAt)
	if err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{
		"api_key": apiKey,
		"message": "API key created successfully",
	})
}

func (hs *HTTPServer) handleListAPIKeys(w http.ResponseWriter, r *http.Request) {
	// Require auth
	if _, err := hs.requireAdmin(w, r); err != nil {
		return
	}
	
	productID := r.URL.Query().Get("product_id")
	product, exists := hs.admin.GetProduct(productID)
	if !exists {
		hs.writeError(w, http.StatusNotFound, "Product not found")
		return
	}
	
	hs.writeJSON(w, product.APIKeys)
}

func (hs *HTTPServer) handleRevokeAPIKey(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ProductID string `json:"product_id"`
		AdminID  string `json:"admin_id"`
		KeyHash  string `json:"key_hash"`
	}
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.RevokeProductAPIKey(req.ProductID, req.AdminID, req.KeyHash); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "API key revoked successfully"})
}

func (hs *HTTPServer) handleCreateAdmin(w http.ResponseWriter, r *http.Request) {
	var req AdminRequest
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	adminID, err := hs.admin.CreateAdmin(req.SuperAdminID, req.Username, req.Email, req.Password)
	if err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{
		"admin_id": adminID,
		"message": "Admin created successfully",
	})
}

func (hs *HTTPServer) handleListAdmins(w http.ResponseWriter, r *http.Request) {
	// Require super admin auth
	if _, err := hs.requireSuperAdmin(w, r); err != nil {
		return
	}
	
	admins := hs.admin.GetAdmins()
	hs.writeJSON(w, admins)
}

func (hs *HTTPServer) handleRemoveAdmin(w http.ResponseWriter, r *http.Request) {
	var req struct {
		SuperAdminID string `json:"super_admin_id"`
		AdminID     string `json:"admin_id"`
	}
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.RemoveAdmin(req.SuperAdminID, req.AdminID); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "Admin removed successfully"})
}

func (hs *HTTPServer) handleGrantPermission(w http.ResponseWriter, r *http.Request) {
	var req PermissionRequest
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.GrantPermissionToAdmin(req.AdminID, req.TargetID, req.Permission); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "Permission granted successfully"})
}

func (hs *HTTPServer) handleRevokePermission(w http.ResponseWriter, r *http.Request) {
	var req PermissionRequest
	if err := hs.readJSON(w, r, &req); err != nil {
		return
	}
	
	if err := hs.admin.RevokePermissionFromAdmin(req.AdminID, req.TargetID, req.Permission); err != nil {
		hs.writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	
	hs.writeJSON(w, map[string]string{"message": "Permission revoked successfully"})
}

func (hs *HTTPServer) handleGetAuditLogs(w http.ResponseWriter, r *http.Request) {
	// Require admin auth
	if _, err := hs.requireAdmin(w, r); err != nil {
		return
	}
	
	limit := 100
	logs := hs.admin.GetAuditLogs(limit)
	hs.writeJSON(w, logs)
}

func (hs *HTTPServer) handleGetStats(w http.ResponseWriter, r *http.Request) {
	stats, _ := hs.admin.GetStats()
	hs.writeJSON(w, stats)
}

// Helper methods
func (hs *HTTPServer) readJSON(w http.ResponseWriter, r *http.Request, v interface{}) error {
	body, err := io.ReadAll(r.Body)
	if err != nil {
		hs.writeError(w, http.StatusBadRequest, "Invalid request body")
		return err
	}
	
	if len(body) == 0 {
		hs.writeError(w, http.StatusBadRequest, "Empty request body")
		return fmt.Errorf("empty body")
	}
	
	if err := json.Unmarshal(body, v); err != nil {
		hs.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return err
	}
	
	return nil
}

func (hs *HTTPServer) writeJSON(w http.ResponseWriter, v interface{}) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(v)
}

func (hs *HTTPServer) writeError(w http.ResponseWriter, code int, message string) {
	w.WriteHeader(code)
	hs.writeJSON(w, map[string]string{"error": message})
}

func (hs *HTTPServer) requireAdmin(w http.ResponseWriter, r *http.Request) (string, error) {
	sessionID := r.Header.Get("Authorization")
	if sessionID == "" {
		hs.writeError(w, http.StatusUnauthorized, "Authorization required")
		return "", fmt.Errorf("unauthorized")
	}
	
	userID, err := hs.admin.VerifySession(sessionID)
	if err != nil {
		hs.writeError(w, http.StatusUnauthorized, "Invalid session")
		return "", err
	}
	
	user, exists := hs.admin.GetUser(userID)
	if !exists || user.Role < RoleAdmin {
		hs.writeError(w, http.StatusForbidden, "Admin role required")
		return "", fmt.Errorf("forbidden")
	}
	
	return userID, nil
}

func (hs *HTTPServer) requireSuperAdmin(w http.ResponseWriter, r *http.Request) (string, error) {
	sessionID := r.Header.Get("Authorization")
	if sessionID == "" {
		hs.writeError(w, http.StatusUnauthorized, "Authorization required")
		return "", fmt.Errorf("unauthorized")
	}
	
	userID, err := hs.admin.VerifySession(sessionID)
	if err != nil {
		hs.writeError(w, http.StatusUnauthorized, "Invalid session")
		return "", err
	}
	
	user, exists := hs.admin.GetUser(userID)
	if !exists || user.Role != RoleSuperAdmin {
		hs.writeError(w, http.StatusForbidden, "Super admin role required")
		return "", fmt.Errorf("forbidden")
	}
	
	return userID, nil
}

func getClientIP(r *http.Request) string {
	// Check X-Forwarded-For header
	if xff := r.Header.Get("X-Forwarded-For"); xff != "" {
		return strings.Split(xff, ",")[0]
	}
	
	// Check X-Real-IP header
	if xri := r.Header.Get("X-Real-IP"); xri != "" {
		return xri
	}
	
	return r.RemoteAddr
}

// Start starts the HTTP server.
func (hs *HTTPServer) Start(addr string) error {
	log.Printf("Admin server starting on %s", addr)
	
	srv := &http.Server{
		Addr:         addr,
		Handler:      hs.mux,
		ReadTimeout:  15 * time.Second,
		WriteTimeout: 15 * time.Second,
		IdleTimeout: 60 * time.Second,
	}
	
	return srv.ListenAndServe()
}

// GetUsers returns all users.
func (wla *WhiteLabelAdmin) GetUsers() []*User {
	wla.mu.RLock()
	defer wla.mu.RUnlock()
	
	users := make([]*User, 0, len(wla.users))
	for _, user := range wla.users {
		users = append(users, user)
	}
	
	return users
}

// GetProducts returns all products.
func (wla *WhiteLabelAdmin) GetProducts() []*Product {
	wla.mu.RLock()
	defer wla.mu.RUnlock()
	
	products := make([]*Product, 0, len(wla.products))
	for _, product := range wla.products {
		products = append(products, product)
	}
	
	return products
}

// GetAdmins returns all admins.
func (wla *WhiteLabelAdmin) GetAdmins() []*User {
	wla.mu.RLock()
	defer wla.mu.RUnlock()
	
	var admins []*User
	for _, user := range wla.users {
		if user.Role >= RoleAdmin {
			admins = append(admins, user)
		}
	}
	
	return admins
}

// String methods
func (r UserRole) String() string {
	switch r {
	case RoleSuperAdmin:
		return "super_admin"
	case RoleAdmin:
		return "admin"
	case RoleWhitelabelClient:
		return "whitelabel_client"
	case RoleUser:
		return "user"
	default:
		return "unknown"
	}
}

func (s UserStatus) String() string {
	switch s {
	case StatusPending:
		return "pending"
	case StatusActive:
		return "active"
	case StatusSuspended:
		return "suspended"
	case StatusBanned:
		return "banned"
	case StatusLocked:
		return "locked"
	default:
		return "unknown"
	}
}