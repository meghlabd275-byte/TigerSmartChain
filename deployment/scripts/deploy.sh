#!/bin/bash
# TigerScan One-Click Deployment Script

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}     TigerScan One-Click Deployment${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"

NETWORK="${NETWORK:-mainnet}"
PORT="${PORT:-8080}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-tigerscan}"
DB_USER="${DB_USER:-tigerscan}"
DB_PASSWORD="${DB_PASSWORD:-}"

check_prerequisites() {
    echo -e "${YELLOW}Checking prerequisites...${NC}"
    local missing=()
    if ! command -v docker &> /dev/null; then missing+=("docker"); fi
    if ! command -v docker compose &> /dev/null && ! command -v docker-compose &> /dev/null; then missing+=("docker-compose"); fi
    if [ ${#missing[@]} -gt 0 ]; then
        echo -e "${RED}Error: Missing: ${missing[*]}${NC}"
        exit 1
    fi
    echo -e "${GREEN}Prerequisites met!${NC}"
}

setup_env() {
    echo -e "${YELLOW}Setting up environment...${NC}"
    if [ ! -f .env ]; then
        cat > .env << EOF
NETWORK=$NETWORK
PORT=$PORT
DB_HOST=$DB_HOST
DB_PORT=$DB_PORT
DB_NAME=$DB_NAME
DB_USER=$DB_USER
DB_PASSWORD=$DB_PASSWORD
ENCRYPTION_KEY=$(openssl rand -hex 32)
JWT_SECRET=$(openssl rand -hex 32)
API_KEY_SALT=$(openssl rand -hex 16)
CHAIN_ID=1
CHAIN_NAME=Ethereum
CHAIN_SYMBOL=ETH
CHAIN_DECIMALS=18
EOF
        echo -e "${GREEN}Created .env file${NC}"
    fi
}

start_services() {
    echo -e "${YELLOW}Starting TigerScan services...${NC}"
    docker compose pull 2>/dev/null || true
    docker compose build --no-cache 2>/dev/null || true
    docker compose up -d
    sleep 10
    check_status
}

check_status() {
    echo -e "${YELLOW}Checking service status...${NC}"
    local max_attempts=30
    local attempt=0
    while [ $attempt -lt $max_attempts ]; do
        if docker compose ps | grep -q "Up"; then
            echo -e "${GREEN}Services are running!${NC}"
            echo ""
            echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
            echo -e "${GREEN}TigerScan is ready!${NC}"
            echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
            echo "API Server:  http://localhost:$PORT"
            echo "Web UI:     http://localhost:$PORT/ui"
            echo "WebSocket:   ws://localhost:$PORT/ws"
            echo "Metrics:   http://localhost:$PORT/metrics"
            echo ""
            echo "API Key:    tsc_admin_key"
            echo ""
            return 0
        fi
        attempt=$((attempt + 1))
        echo -n "."
        sleep 2
    done
    echo -e "${RED}Failed to start. Check: docker compose logs${NC}"
    return 1
}

stop_services() {
    echo -e "${YELLOW}Stopping TigerScan...${NC}"
    docker compose down
    echo -e "${GREEN}Stopped${NC}"
}

reset() {
    echo -e "${YELLOW}Resetting TigerScan...${NC}"
    docker compose down -v
    rm -f .env
    echo -e "${GREEN}Reset complete${NC}"
}

upgrade() {
    echo -e "${YELLOW}Upgrading TigerScan...${NC}"
    docker compose pull
    docker compose up -d
    echo -e "${GREEN}Upgrade complete${NC}"
}

case "${1:-start}" in
    start)
        check_prerequisites
        setup_env
        start_services
        ;;
    stop) stop_services ;;
    restart) stop_services; start_services ;;
    status) docker compose ps ;;
    logs) docker compose logs -f --tail=100 ;;
    reset) reset ;;
    upgrade) upgrade ;;
    *) echo "Usage: $0 {start|stop|restart|status|logs|reset|upgrade}" ;;
esac