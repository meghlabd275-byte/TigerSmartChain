#!/bin/bash
# TigerScan Backup Script
# Automated backup for database and configuration

set -e

BACKUP_DIR="${BACKUP_DIR:-/backups}"
RETENTION_DAYS="${RETENTION_DAYS:-30}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-tigerscan}"
DB_USER="${DB_USER:-tigerscan}"
S3_BUCKET="${S3_BUCKET:-}"
ENCRYPTION_KEY="${ENCRYPTION_KEY:-}"

timestamp() {
    date +"%Y%m%d_%H%M%S"
}

echo "TigerScan Backup - $(timestamp)"

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Backup database
backup_database() {
    echo "Backing up database..."
    
    local backup_file="$BACKUP_DIR/db_$(timestamp).sql.gz.enc"
    
    # Dump and compress
    pg_dump -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -Fc "$DB_NAME" | \
        gzip | \
        openssl enc -aes-256-cbc -salt -pbkdf2 -pass pass:"$ENCRYPTION_KEY" > "$backup_file"
    
    echo "Database backup: $backup_file"
    
    # Verify backup
    if [ -f "$backup_file" ] && [ -s "$backup_file" ]; then
        echo "Database backup complete"
    else
        echo "Database backup failed"
        exit 1
    fi
}

# Backup configuration
backup_config() {
    echo "Backing up configuration..."
    
    local config_file="$BACKUP_DIR/config_$(timestamp).tar.gz.enc"
    
    tar -czf - \
        -C /workspace/project/TigerSmartChain \
        .env docker-compose.yml deployment/ | \
        openssl enc -aes-256-cbc -salt -pbkdf2 -pass pass:"$ENCRYPTION_KEY" > "$config_file"
    
    echo "Config backup: $config_file"
}

# Backup blockchain data (optional - large)
backup_blockchain() {
    if [ "$BACKUP_BLOCKCHAIN" = "true" ]; then
        echo "Backing up blockchain data..."
        
        local chain_file="$BACKUP_DIR/blockchain_$(timestamp).tar.gz.enc"
        
        tar -czf - -C /data blockchain/ 2>/dev/null | \
            openssl enc -aes-256-cbc -salt -pbkdf2 -pass pass:"$ENCRYPTION_KEY" > "$chain_file"
        
        echo "Blockchain backup: $chain_file"
    fi
}

# Upload to S3
upload_s3() {
    if [ -n "$S3_BUCKET" ]; then
        echo "Uploading to S3..."
        
        aws s3 sync "$BACKUP_DIR/" "s3://$S3_BUCKET/backups/" --storage-class STANDARD_IA
        
        echo "Upload complete"
    fi
}

# Cleanup old backups
cleanup() {
    echo "Cleaning up old backups..."
    
    find "$BACKUP_DIR" -type f -mtime +"$RETENTION_DAYS" -delete
    
    echo "Cleanup complete"
}

# Verify backup
verify() {
    local backup_file="$1"
    
    if openssl enc -aes-256-cbc -d -pbkdf2 -pass pass:"$ENCRYPTION_KEY" -in "$backup_file" > /dev/null 2>&1; then
        echo "Backup verified: $backup_file"
        return 0
    else
        echo "Backup verification failed: $backup_file"
        return 1
    fi
}

# Main
case "${1:-full}" in
    full)
        backup_database
        backup_config
        upload_s3
        cleanup
        ;;
    database|db)
        backup_database
        upload_s3
        ;;
    config|cfg)
        backup_config
        upload_s3
        ;;
    cleanup)
        cleanup
        ;;
    *)
        echo "Usage: $0 {full|database|config|cleanup}"
        ;;
esac

echo "Backup complete - $(timestamp)"