#!/bin/bash
# TigerScan Automated Backup System
# Production-grade backup with encryption and integrity verification

set -euo pipefail

# Configuration
S3_BUCKET="${S3_BACKUP_BUCKET:-tigerscan-backups}"
POSTGRES_HOST="${POSTGRES_HOST:-localhost}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
POSTGRES_DB="${POSTGRES_DB:-tigerscan}"
POSTGRES_USER="${POSTGRES_USER:-tigerscan}"
REDIS_HOST="${REDIS_HOST:-localhost}"
REDIS_PORT="${REDIS_PORT:-6379}"
ELASTICSEARCH_HOSTS="${ELASTICSEARCH_HOSTS:-localhost:9200}"
BACKUP_DIR="${BACKUP_DIR:-/var/backups/tigerscan}"
ENCRYPTION_KEY="${BACKUP_ENCRYPTION_KEY:-}"
GPG_RECIPIENT="${GPG_RECIPIENT:-backup@tigerscan.io}"
RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-30}"
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_NAME="tigerscan_backup_${DATE}"

# Logging
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a /var/log/tigerscan/backup.log
}

error() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] ERROR: $*" | tee -a /var/log/tigerscan/backup.log
    exit 1
}

# Check prerequisites
check_prerequisites() {
    log "Checking prerequisites..."
    
    command -v psql >/dev/null 2>&1 || error "psql not found"
    command -v redis-cli >/dev/null 2>&1 || error "redis-cli not found"
    command -v gzip >/dev/null 2>&1 || error "gzip not found"
    command -v openssl >/dev/null 2>&1 || error "openssl not found"
    
    if [ -n "$ENCRYPTION_KEY" ]; then
        command -v gpg >/dev/null 2>&1 || error "gpg not found"
    fi
    
    # Check database connectivity
    psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c "SELECT 1" >/dev/null 2>&1 || error "Cannot connect to PostgreSQL"
    
    log "Prerequisites check passed"
}

# Create backup directory
create_backup_dir() {
    log "Creating backup directory..."
    mkdir -p "${BACKUP_DIR}/${BACKUP_NAME}"
    mkdir -p "${BACKUP_DIR}/${BACKUP_NAME}/postgresql"
    mkdir -p "${BACKUP_DIR}/${BACKUP_NAME}/redis"
    mkdir -p "${BACKUP_DIR}/${BACKUP_NAME}/elasticsearch"
    mkdir -p "${BACKUP_DIR}/${BACKUP_NAME}/configs"
    mkdir -p "${BACKUP_DIR}/${BACKUP_NAME}/blockchain"
}

# Backup PostgreSQL
backup_postgresql() {
    log "Starting PostgreSQL backup..."
    
    local dump_file="${BACKUP_DIR}/${BACKUP_NAME}/postgresql/dump.sql"
    local compressed_file="${BACKUP_DIR}/${BACKUP_NAME}/postgresql/dump.sql.gz"
    local checksum_file="${BACKUP_DIR}/${BACKUP_NAME}/postgresql/dump.sql.gz.sha256"
    
    # Create pg_dump with custom format for best compression
    pg_dump -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
        --format=custom \
        --compress=9 \
        --verbose \
        --file="$dump_file" || error "pg_dump failed"
    
    # Calculate checksum
    sha256sum "$dump_file" > "$dump_file.sha256"
    
    # Encrypt if key provided
    if [ -n "$ENCRYPTION_KEY" ]; then
        log "Encrypting PostgreSQL backup..."
        openssl enc -aes-256-cbc -salt -pbkdf2 -in "$dump_file" -out "$compressed_file" -pass pass:"$ENCRYPTION_KEY" || error "Encryption failed"
        rm -f "$dump_file"
        compressed_file="$dump_file.enc"
    else
        gzip -c "$dump_file" > "$compressed_file"
        rm -f "$dump_file"
    fi
    
    # Calculate final checksum
    sha256sum "$compressed_file" > "$checksum_file"
    
    local size=$(du -h "$compressed_file" | cut -f1)
    log "PostgreSQL backup completed: $size"
}

# Backup Redis
backup_redis() {
    log "Starting Redis backup..."
    
    local dump_file="${BACKUP_DIR}/${BACKUP_NAME}/redis/dump.rdb"
    local backup_file="${BACKUP_DIR}/${BACKUP_NAME}/redis/dump.json"
    local compressed_file="${BACKUP_DIR}/${BACKUP_NAME}/redis/dump.json.gz"
    local checksum_file="${BACKUP_DIR}/${BACKUP_NAME}/redis/dump.json.gz.sha256"
    
    # Get all keys with values
    redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" --scan | while read key; do
        local key_type=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" type "$key")
        if [ "$key_type" = "string" ]; then
            local value=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" get "$key")
            echo "{\"key\":\"$key\",\"type\":\"string\",\"value\":\"$value\"}" >> "$backup_file"
        elif [ "$key_type" = "hash" ]; then
            local fields=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" hgetall "$key")
            echo "{\"key\":\"$key\",\"type\":\"hash\",\"fields\":$fields}" >> "$backup_file"
        elif [ "$key_type" = "list" ]; then
            local values=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" lrange "$key" 0 -1)
            echo "{\"key\":\"$key\",\"type\":\"list\",\"values\":$values}" >> "$backup_file"
        elif [ "$key_type" = "set" ]; then
            local members=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" smembers "$key")
            echo "{\"key\":\"$key\",\"type\":\"set\",\"members\":$members}" >> "$backup_file"
        fi
    done
    
    # Compress
    gzip -c "$backup_file" > "$compressed_file"
    rm -f "$backup_file"
    
    # Encrypt if key provided
    if [ -n "$ENCRYPTION_KEY" ]; then
        log "Encrypting Redis backup..."
        mv "$compressed_file" "${compressed_file}.gz"
        openssl enc -aes-256-cbc -salt -pbkdf2 -in "${compressed_file}.gz" -out "$compressed_file" -pass pass:"$ENCRYPTION_KEY" || error "Encryption failed"
        rm -f "${compressed_file}.gz"
    fi
    
    # Calculate checksum
    sha256sum "$compressed_file" > "$checksum_file"
    
    local size=$(du -h "$compressed_file" | cut -f1)
    log "Redis backup completed: $size"
}

# Backup Elasticsearch indices
backup_elasticsearch() {
    log "Starting Elasticsearch backup..."
    
    local backup_file="${BACKUP_DIR}/${BACKUP_NAME}/elasticsearch/snapshot.json"
    local compressed_file="${BACKUP_DIR}/${BACKUP_NAME}/elasticsearch/snapshot.json.gz"
    
    # Create repository if not exists
    curl -s -XPUT "http://${ELASTICSEARCH_HOSTS}/_snapshot/tigerscan_backup" -H 'Content-Type: application/json' -d "{
        \"type\": \"fs\",
        \"settings\": {
            \"location\": \"${BACKUP_DIR}/${BACKUP_NAME}/elasticsearch\"
        }
    }" || true
    
    # Create snapshot
    curl -s -XPUT "http://${ELASTICSEARCH_HOSTS}/_snapshot/tigerscan_backup/snapshot_${DATE}?wait_for_completion=true" | tee "$backup_file"
    
    # Compress
    gzip -c "$backup_file" > "$compressed_file"
    rm -f "$backup_file"
    
    # Calculate checksum
    sha256sum "$compressed_file" > "${compressed_file}.sha256"
    
    local size=$(du -h "$compressed_file" | cut -f1)
    log "Elasticsearch backup completed: $size"
}

# Backup blockchain data
backup_blockchain() {
    log "Starting blockchain data backup..."
    
    local backup_dir="${BACKUP_DIR}/${BACKUP_NAME}/blockchain"
    
    # Backup leveldb data if exists
    if [ -d "/var/lib/tigersmartchain/leveldb" ]; then
        log "Backing up LevelDB..."
        tar -czf "${backup_dir}/leveldb.tar.gz" -C /var/lib/tigersmartchain leveldb || true
    fi
    
    # Backup genesis file
    if [ -f "/var/lib/tigersmartchain/genesis.json" ]; then
        cp /var/lib/tigersmartchain/genesis.json "${backup_dir}/" || true
    fi
    
    # Backup chain config
    if [ -d "/var/lib/tigersmartchain/config" ]; then
        tar -czf "${backup_dir}/config.tar.gz" -C /var/lib/tigersmartchain config || true
    fi
    
    log "Blockchain data backup completed"
}

# Backup configuration files
backup_configs() {
    log "Starting configuration backup..."
    
    local config_dir="${BACKUP_DIR}/${BACKUP_NAME}/configs"
    
    # Copy important configs (read-only, no secrets)
    cp /etc/tigerscan/api-server.yaml "${config_dir}/" 2>/dev/null || true
    cp /etc/tigerscan/indexer.yaml "${config_dir}/" 2>/dev/null || true
    cp /etc/tigerscan/graphql.yaml "${config_dir}/" 2>/dev/null || true
    
    log "Configuration backup completed"
}

# Create manifest
create_manifest() {
    log "Creating backup manifest..."
    
    local manifest_file="${BACKUP_DIR}/${BACKUP_NAME}/manifest.json"
    
    cat > "$manifest_file" << EOF
{
    "backup_name": "${BACKUP_NAME}",
    "timestamp": "$(date -Iseconds)",
    "hostname": "$(hostname)",
    "postgresql_version": "$(psql -h "$POSTGRES_HOST" -p "$POSTGRES_PORT" -U "$POSTGRES_USER" -d "$POSTGRES_DB" -t -c 'SELECT version()' 2>/dev/null | head -1)",
    "redis_version": "$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" INFO server 2>/dev/null | grep redis_version | cut -d: -f2)",
    "retention_days": ${RETENTION_DAYS},
    "encrypted": $([ -n "$ENCRYPTION_KEY" ] && echo "true" || echo "false"),
    "components": [
        "postgresql",
        "redis", 
        "elasticsearch",
        "blockchain",
        "configs"
    ]
}
EOF
    
    # Sign manifest
    if [ -n "$GPG_RECIPIENT" ]; then
        echo "$GPG_RECIPIENT" | xargs gpg --armor --encrypt -o "${manifest_file}.asc" "$manifest_file" || true
    fi
    
    log "Manifest created"
}

# Upload to S3
upload_to_s3() {
    log "Uploading to S3..."
    
    if command -v aws >/dev/null 2>&1 && [ -n "${AWS_ACCESS_KEY_ID:-}" ]; then
        aws s3 sync "${BACKUP_DIR}/${BACKUP_NAME}/" "s3://${S3_BUCKET}/${BACKUP_NAME}/" --storage-class STANDARD_IA || error "S3 upload failed"
        log "Uploaded to S3://s3://${S3_BUCKET}/${BACKUP_NAME}/"
    else
        log "S3 upload skipped (AWS CLI not configured)"
    fi
}

# Cleanup old backups
cleanup_old_backups() {
    log "Cleaning up old backups (retention: ${RETENTION_DAYS} days)..."
    
    # Local cleanup
    find "$BACKUP_DIR" -maxdepth 1 -type d -name "tigerscan_backup_*" -mtime +"$RETENTION_DAYS" -exec rm -rf {} \; 2>/dev/null || true
    
    # S3 cleanup
    if command -v aws >/dev/null 2>&1 && [ -n "${AWS_ACCESS_KEY_ID:-}" ]; then
        aws s3 ls "s3://${S3_BUCKET}/" | grep "tigerscan_backup_" | while read -r line; do
            local backup_date=$(echo "$line" | awk '{print $1}')
            local backup_name=$(echo "$line" | awk '{print $2}')
            local backup_age=$(echo "$(date +%s) - $(date -d "$backup_date" +%s)")
            if [ "$backup_age" -gt "$((RETENTION_DAYS * 86400))" ]; then
                aws s3 rm "s3://${S3_BUCKET}/${backup_name}" --recursive || true
            fi
        done
    fi
    
    log "Old backups cleaned up"
}

# Verify backup integrity
verify_backup() {
    log "Verifying backup integrity..."
    
    local backup_dir="${BACKUP_DIR}/${BACKUP_NAME}"
    local verified=true
    
    # Verify checksums
    find "$backup_dir" -name "*.sha256" -exec sh -c '
        for file in "$@"; do
            if ! sha256 -c "$file" 2>/dev/null; then
                echo "Checksum verification failed for $file"
                exit 1
            fi
        done
    ' _ {} + || verified=false
    
    if [ "$verified" = "true" ]; then
        log "Backup integrity verified"
    else
        error "Backup integrity check failed"
    fi
}

# Main execution
main() {
    log "========================================="
    log "TigerScan Backup Starting"
    log "========================================="
    
    check_prerequisites
    create_backup_dir
    backup_postgresql
    backup_redis
    backup_elasticsearch
    backup_blockchain
    backup_configs
    create_manifest
    verify_backup
    upload_to_s3
    cleanup_old_backups
    
    log "========================================="
    log "TigerScan Backup Completed Successfully"
    log "========================================="
}

main "$@"