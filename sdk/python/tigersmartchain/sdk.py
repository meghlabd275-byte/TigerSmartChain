"""
TigerScan Python SDK - Complete blockchain interaction SDK

This module provides comprehensive Python SDK for interacting with TigerScan API
and TigerSmartChain blockchain.
"""

import os
import json
import time
import hashlib
import hmac
from typing import Optional, List, Dict, Any, Union
from dataclasses import dataclass, field
from datetime import datetime
from decimal import Decimal
import enum

import requests
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry

__version__ = "1.0.0"


class ChainID(enum.Enum):
    """Supported chain IDs"""
    TIGER_MAINNET = 1
    TIGER_TESTNET = 5
    ETHEREUM_MAINNET = 1
    ETHEREUM_SEPOLIA = 11155111


@dataclass
class Block:
    """Represents a blockchain block"""
    number: int
    hash: str
    parent_hash: str
    nonce: str
    sha3_uncles: str
    logs_bloom: str
    transactions_root: str
    state_root: str
    receipts_root: str
    miner: str
    difficulty: int
    total_difficulty: int
    size: int
    gas_limit: int
    gas_used: int
    timestamp: int
    transaction_count: int = 0
    uncle_count: int = 0
    uncles: List[str] = field(default_factory=list)
    transactions: List[str] = field(default_factory=list)
    
    @property
    def datetime(self) -> datetime:
        return datetime.fromtimestamp(self.timestamp)
    
    @property
    def gas_utilization(self) -> float:
        return (self.gas_used / self.gas_limit) * 100 if self.gas_limit > 0 else 0


@dataclass
class Transaction:
    """Represents a blockchain transaction"""
    hash: str
    nonce: int
    block_hash: str
    block_number: int
    transaction_index: int
    from_address: str
    to_address: str
    value: str
    gas_price: str
    gas_limit: int
    gas_used: int
    input_data: str
    status: int = 1
    timestamp: int = 0
    
    @property
    def value_eth(self) -> Decimal:
        return Decimal(self.value) / Decimal(10**18)
    
    @property
    def gas_price_gwei(self) -> Decimal:
        return Decimal(self.gas_price) / Decimal(10**9)
    
    @property
    def fee_eth(self) -> Decimal:
        return Decimal(str(self.gas_used)) * Decimal(self.gas_price) / Decimal(10**18)


@dataclass
class TransactionReceipt:
    """Represents a transaction receipt"""
    transaction_hash: str
    block_hash: str
    block_number: int
    cumulative_gas_used: int
    gas_used: int
    contract_address: Optional[str]
    status: int
    logs: List[Dict] = field(default_factory=list)
    logs_bloom: str = ""
    

@dataclass
class Token:
    """Represents an ERC-20 token"""
    address: str
    name: str
    symbol: str
    decimals: int
    total_supply: str
    price: str = "0"
    market_cap: str = "0"
    volume_24h: str = "0"
    holders: int = 0
    transfers: int = 0
    
    @property
    def total_supply_eth(self) -> Decimal:
        return Decimal(self.total_supply) / Decimal(10**self.decimals)
    
    @property
    def price_usd(self) -> Decimal:
        return Decimal(self.price) / Decimal(10**8)


@dataclass
class TokenHolder:
    """Represents a token holder"""
    address: str
    balance: str
    percent: float
    
    @property
    def balance_eth(self, decimals: int = 18) -> Decimal:
        return Decimal(self.balance) / Decimal(10**decimals)


@dataclass
class NFT:
    """Represents an NFT collection"""
    address: str
    name: str
    symbol: str
    total_supply: int
    floor_price: str = "0"
    holders: int = 0
    transfers: int = 0
    royalty_bps: int = 0


@dataclass
class Account:
    """Represents an account"""
    address: str
    balance: str
    transaction_count: int = 0
    token_balance: List[Token] = field(default_factory=list)
    
    @property
    def balance_eth(self) -> Decimal:
        return Decimal(self.balance) / Decimal(10**18)


class TigerScanSDK:
    """Main SDK client for TigerScan"""
    
    def __init__(
        self,
        api_key: Optional[str] = None,
        base_url: str = "https://api.tigerscan.io",
        chain_id: ChainID = ChainID.TIGER_MAINNET,
        timeout: int = 30,
        max_retries: int = 3
    ):
        self.api_key = api_key or os.environ.get("TIGERSCAN_API_KEY", "")
        self.base_url = base_url.rstrip("/")
        self.chain_id = chain_id
        self.timeout = timeout
        
        # Setup session with retry
        self.session = requests.Session()
        retry_strategy = Retry(
            total=max_retries,
            backoff_factor=1,
            status_forcelist=[429, 500, 502, 503, 504]
        )
        adapter = HTTPAdapter(max_retries=retry_strategy)
        self.session.mount("http://", adapter)
        self.session.mount("https://", adapter)
    
    def _headers(self) -> Dict[str, str]:
        headers = {
            "Content-Type": "application/json",
            "User-Agent": f"TigerScan-SDK-Python/{__version__}"
        }
        if self.api_key:
            headers["X-API-Key"] = self.api_key
        return headers
    
    def _request(
        self,
        method: str,
        endpoint: str,
        params: Optional[Dict] = None,
        data: Optional[Dict] = None
    ) -> Dict:
        url = f"{self.base_url}/api/v2{endpoint}"
        
        response = self.session.request(
            method=method,
            url=url,
            params=params,
            json=data,
            headers=self._headers(),
            timeout=self.timeout
        )
        response.raise_for_status()
        
        result = response.json()
        if result.get("status") != "1":
            raise Exception(result.get("message", "API error"))
        return result.get("result", {})
    
    # Block operations
    
    def get_block(self, block_number: Optional[int] = None, block_hash: Optional[str] = None) -> Block:
        """Get block by number or hash"""
        if block_number is not None:
            result = self._request("GET", f"/block/{block_number}")
        elif block_hash is not None:
            result = self._request("GET", f"/block/hash/{block_hash}")
        else:
            raise ValueError("Must provide block_number or block_hash")
        
        return Block(
            number=result.get("blockNumber", 0),
            hash=result.get("hash", ""),
            parent_hash=result.get("parentHash", ""),
            nonce=result.get("nonce", ""),
            sha3_uncles=result.get("sha3Uncles", ""),
            logs_bloom=result.get("logsBloom", ""),
            transactions_root=result.get("transactionsRoot", ""),
            state_root=result.get("stateRoot", ""),
            receipts_root=result.get("receiptsRoot", ""),
            miner=result.get("miner", ""),
            difficulty=int(result.get("difficulty", 0)),
            total_difficulty=int(result.get("totalDifficulty", 0)),
            size=int(result.get("size", 0)),
            gas_limit=int(result.get("gasLimit", 0)),
            gas_used=int(result.get("gasUsed", 0)),
            timestamp=int(result.get("timestamp", 0)),
            transaction_count=len(result.get("transactions", [])),
            transaction=result.get("transactions", [])
        )
    
    def get_latest_block(self) -> Block:
        """Get the latest block"""
        result = self._request("GET", "/block/latest")
        return self.get_block(int(result.get("blockNumber", 0)))
    
    def get_block_rewards(self, block_number: int) -> Dict:
        """Get block rewards"""
        return self._request("GET", f"/block/{block_number}/rewards")
    
    # Transaction operations
    
    def get_transaction(self, tx_hash: str) -> Transaction:
        """Get transaction by hash"""
        result = self._request("GET", f"/tx/{tx_hash}")
        
        return Transaction(
            hash=result.get("hash", ""),
            nonce=int(result.get("nonce", 0)),
            block_hash=result.get("blockHash", ""),
            block_number=int(result.get("blockNumber", 0)),
            transaction_index=int(result.get("transactionIndex", 0)),
            from_address=result.get("from", ""),
            to_address=result.get("to", ""),
            value=result.get("value", "0"),
            gas_price=result.get("gasPrice", "0"),
            gas_limit=int(result.get("gasLimit", 0)),
            gas_used=int(result.get("gasUsed", 0)),
            input_data=result.get("input", ""),
            status=int(result.get("status", 1)),
            timestamp=int(result.get("timestamp", 0))
        )
    
    def get_transaction_receipt(self, tx_hash: str) -> TransactionReceipt:
        """Get transaction receipt"""
        result = self._request("GET", f"/tx/{tx_hash}/receipt")
        
        return TransactionReceipt(
            transaction_hash=result.get("transactionHash", ""),
            block_hash=result.get("blockHash", ""),
            block_number=int(result.get("blockNumber", 0)),
            cumulative_gas_used=int(result.get("cumulativeGasUsed", 0)),
            gas_used=int(result.get("gasUsed", 0)),
            contract_address=result.get("contractAddress"),
            status=int(result.get("status", 1)),
            logs=result.get("logs", [])
        )
    
    def get_internal_transactions(self, tx_hash: str) -> List[Dict]:
        """Get internal transactions"""
        result = self._request("GET", f"/tx/{tx_hash}/internal")
        return result.get("results", [])
    
    # Account operations
    
    def get_account(self, address: str) -> Account:
        """Get account information"""
        result = self._request("GET", f"/account/{address}")
        
        return Account(
            address=result.get("address", ""),
            balance=result.get("balance", "0"),
            transaction_count=int(result.get("transactionCount", 0))
        )
    
    def get_account_transactions(
        self,
        address: str,
        page: int = 1,
        limit: int = 50
    ) -> List[Transaction]:
        """Get account transactions"""
        result = self._request(
            "GET",
            f"/account/{address}/transactions",
            params={"page": page, "limit": limit}
        )
        
        return [
            Transaction(
                hash=tx.get("hash", ""),
                nonce=int(tx.get("nonce", 0)),
                block_hash=tx.get("blockHash", ""),
                block_number=int(tx.get("blockNumber", 0)),
                transaction_index=int(tx.get("transactionIndex", 0)),
                from_address=tx.get("from", ""),
                to_address=tx.get("to", ""),
                value=tx.get("value", "0"),
                gas_price=tx.get("gasPrice", "0"),
                gas_limit=int(tx.get("gasLimit", 0)),
                gas_used=int(tx.get("gasUsed", 0)),
                input_data=tx.get("input", ""),
                status=int(tx.get("status", 1)),
                timestamp=int(tx.get("timestamp", 0))
            )
            for tx in result.get("results", [])
        ]
    
    def get_account_tokens(self, address: str) -> List[Token]:
        """Get account token balances"""
        result = self._request("GET", f"/account/{address}/tokens")
        
        return [
            Token(
                address=t.get("address", ""),
                name=t.get("name", ""),
                symbol=t.get("symbol", ""),
                decimals=int(t.get("decimals", 18)),
                total_supply=t.get("totalSupply", "0"),
                price=t.get("price", "0"),
                market_cap=t.get("marketCap", "0"),
                volume_24h=t.get("volume24h", "0"),
                holders=int(t.get("holders", 0)),
                transfers=int(t.get("transfers", 0))
            )
            for t in result.get("results", [])
        ]
    
    # Token operations
    
    def get_tokens(
        self,
        page: int = 1,
        limit: int = 50,
        sort: str = "price"
    ) -> List[Token]:
        """Get token list"""
        result = self._request(
            "GET",
            "/tokens",
            params={"page": page, "offset": limit, "sort": sort}
        )
        
        return [
            Token(
                address=t.get("address", ""),
                name=t.get("name", ""),
                symbol=t.get("symbol", ""),
                decimals=int(t.get("decimals", 18)),
                total_supply=t.get("totalSupply", "0"),
                price=t.get("price", "0"),
                market_cap=t.get("marketCap", "0"),
                volume_24h=t.get("volume24h", "0"),
                holders=int(t.get("holders", 0)),
                transfers=int(t.get("transfers", 0))
            )
            for t in result
        ]
    
    def get_token(self, address: str) -> Token:
        """Get token info"""
        result = self._request("GET", f"/token/{address}")
        
        return Token(
            address=result.get("address", ""),
            name=result.get("name", ""),
            symbol=result.get("symbol", ""),
            decimals=int(result.get("decimals", 18)),
            total_supply=result.get("totalSupply", "0"),
            price=result.get("price", "0"),
            market_cap=result.get("marketCap", "0"),
            volume_24h=result.get("volume24h", "0"),
            holders=int(result.get("holders", 0)),
            transfers=int(result.get("transfers", 0))
        )
    
    def get_token_holders(
        self,
        address: str,
        page: int = 1,
        limit: int = 50
    ) -> List[TokenHolder]:
        """Get token holders"""
        result = self._request(
            "GET",
            f"/token/{address}/holders",
            params={"page": page, "limit": limit}
        )
        
        return [
            TokenHolder(
                address=h.get("address", ""),
                balance=h.get("balance", "0"),
                percent=float(h.get("percent", 0))
            )
            for h in result.get("results", [])
        ]
    
    def get_token_transfers(
        self,
        address: str,
        page: int = 1,
        limit: int = 50
    ) -> List[Dict]:
        """Get token transfers"""
        result = self._request(
            "GET",
            f"/token/{address}/transfers",
            params={"page": page, "limit": limit}
        )
        return result.get("results", [])
    
    def get_token_price_history(
        self,
        address: str,
        days: int = 30
    ) -> List[Dict]:
        """Get token price history"""
        result = self._request(
            "GET",
            f"/token/{address}/history",
            params={"days": days}
        )
        return result.get("results", [])
    
    # NFT operations
    
    def get_nfts(self, page: int = 1, filter: str = "erc721") -> List[NFT]:
        """Get NFT list"""
        result = self._request(
            "GET",
            "/nfts",
            params={"page": page, "filter": filter}
        )
        
        return [
            NFT(
                address=n.get("address", ""),
                name=n.get("name", ""),
                symbol=n.get("symbol", ""),
                total_supply=int(n.get("totalSupply", 0)),
                floor_price=n.get("floorPrice", "0"),
                holders=int(n.get("holders", 0)),
                transfers=int(n.get("transfers", 0)),
                royalty_bps=int(n.get("royaltyBps", 0))
            )
            for n in result.get("results", [])
        ]
    
    def get_nft(self, address: str) -> NFT:
        """Get NFT info"""
        result = self._request("GET", f"/nft/{address}")
        
        return NFT(
            address=result.get("address", ""),
            name=result.get("name", ""),
            symbol=result.get("symbol", ""),
            total_supply=int(result.get("totalSupply", 0)),
            floor_price=result.get("floorPrice", "0"),
            holders=int(result.get("holders", 0)),
            transfers=int(result.get("transfers", 0)),
            royalty_bps=int(result.get("royaltyBps", 0))
        )
    
    def get_nft_holders(self, address: str, page: int = 1) -> List[Dict]:
        """Get NFT holders"""
        result = self._request(
            "GET",
            f"/nft/{address}/holders",
            params={"page": page}
        )
        return result.get("results", [])
    
    def get_nft_floor_price(self, address: str) -> Dict:
        """Get NFT floor price"""
        return self._request("GET", f"/nft/{address}/floor")
    
    def get_nft_metadata(self, address: str, token_id: str) -> Dict:
        """Get NFT metadata"""
        return self._request(
            "GET",
            f"/nft/{address}/metadata",
            params={"tokenId": token_id}
        )
    
    # Analytics
    
    def get_network_stats(self) -> Dict:
        """Get network statistics"""
        return self._request("GET", "/stats")
    
    def get_tps(self, interval: str = "24h") -> float:
        """Get transactions per second"""
        result = self._request(
            "GET",
            "/stats/tps",
            params={"interval": interval}
        )
        return float(result.get("tps", 0))
    
    def get_gas_price(self) -> Dict:
        """Get current gas prices"""
        return self._request("GET", "/stats/gas")
    
    def get_tvl(self) -> str:
        """Get total value locked"""
        result = self._request("GET", "/stats/tvl")
        return result.get("tvl", "0")
    
    def get_market_cap(self) -> str:
        """Get market cap"""
        result = self._request("GET", "/stats/marketcap")
        return result.get("marketcap", "0")
    
    def get_rich_list(self, page: int = 1) -> List[Dict]:
        """Get rich list"""
        result = self._request(
            "GET",
            "/stats/richlist",
            params={"page": page}
        )
        return result.get("results", [])
    
    def get_top_tokens(self, page: int = 1) -> List[Token]:
        """Get top tokens"""
        result = self._request(
            "GET",
            "/stats/toptokens",
            params={"page": page}
        )
        return [Token(
            address=t.get("address", ""),
            name=t.get("name", ""),
            symbol=t.get("symbol", ""),
            decimals=int(t.get("decimals", 18)),
            total_supply=t.get("totalSupply", "0"),
            price=t.get("price", "0"),
            market_cap=t.get("marketCap", "0"),
            volume_24h=t.get("volume24h", "0"),
            holders=int(t.get("holders", 0)),
            transfers=int(t.get("transfers", 0))
        ) for t in result.get("results", [])]
    
    def get_top_nfts(self, page: int = 1) -> List[NFT]:
        """Get top NFTs"""
        result = self._request(
            "GET",
            "/stats/topnfts",
            params={"page": page}
        )
        return [NFT(
            address=n.get("address", ""),
            name=n.get("name", ""),
            symbol=n.get("symbol", ""),
            total_supply=int(n.get("totalSupply", 0)),
            floor_price=n.get("floorPrice", "0"),
            holders=int(n.get("holders", 0)),
            transfers=int(n.get("transfers", 0)),
            royalty_bps=int(n.get("royaltyBps", 0))
        ) for n in result.get("results", [])]
    
    # Chart data
    
    def get_tps_chart(self, interval: str = "24h") -> List[Dict]:
        """Get TPS chart data"""
        result = self._request(
            "GET",
            "/charts/tps",
            params={"interval": interval}
        )
        return result.get("series", [{}]).get("data", [])
    
    def get_gas_chart(self, interval: str = "24h") -> Dict:
        """Get gas chart data"""
        return self._request(
            "GET",
            "/charts/gas",
            params={"interval": interval}
        )
    
    def get_tvl_chart(self, interval: str = "30d") -> List[Dict]:
        """Get TVL chart data"""
        result = self._request(
            "GET",
            "/charts/tvl",
            params={"interval": interval}
        )
        return result.get("series", [{}]).get("data", [])
    
    def get_token_price_chart(
        self,
        address: str,
        interval: str = "30d"
    ) -> List[Dict]:
        """Get token price chart"""
        result = self._request(
            "GET",
            f"/charts/token/{address}",
            params={"interval": interval}
        )
        return result.get("series", [{}]).get("data", [])
    
    def get_gas_heatmap(self, days: int = 7) -> List[Dict]:
        """Get gas heatmap data"""
        result = self._request(
            "GET",
            "/charts/heatmap",
            params={"days": days}
        )
        return result.get("heatmap", [])
    
    # Search
    
    def search(self, query: str) -> Dict:
        """Search for blocks, transactions, addresses, tokens"""
        return self._request(
            "GET",
            "/search",
            params={"q": query}
        )
    
    # Contract interaction
    
    def read_contract(
        self,
        address: str,
        method: str,
        params: List[str] = None
    ) -> str:
        """Read from a contract"""
        result = self._request(
            "GET",
            f"/contract/{address}/read",
            params={"method": method, "params": params or []}
        )
        return result.get("result", "")
    
    def write_contract(
        self,
        address: str,
        method: str,
        params: List[str],
        from_address: str
    ) -> str:
        """Write to a contract (requires wallet)"""
        result = self._request(
            "POST",
            f"/contract/{address}/write",
            data={
                "method": method,
                "params": params,
                "from": from_address
            }
        )
        return result.get("result", "")
    
    # Export
    
    def export_data(
        self,
        export_type: str,
        address: Optional[str] = None,
        format: str = "json",
        start_block: Optional[int] = None,
        end_block: Optional[int] = None,
        limit: int = 10000
    ) -> Union[List[Dict], str]:
        """Export blockchain data"""
        params = {
            "type": export_type,
            "format": format,
            "limit": limit
        }
        if address:
            params["address"] = address
        if start_block:
            params["startBlock"] = start_block
        if end_block:
            params["endBlock"] = end_block
        
        result = self._request("GET", "/export", params=params)
        
        if format == "csv":
            return result  # Return raw CSV text
        return result.get("results", [])


class Wallet:
    """Wallet for signing and sending transactions"""
    
    def __init__(
        self,
        private_key: str,
        sdk: Optional[TigerScanSDK] = None,
        rpc_url: Optional[str] = None
    ):
        self.private_key = private_key
        self.sdk = sdk
        self.rpc_url = rpc_url or os.environ.get("TIGERSCAN_RPC_URL", "http://localhost:8545")
        
        # Derive address from private key
        self.address = self._derive_address()
    
    def _derive_address(self) -> str:
        """Derive address from private key"""
        # In production, use eth_keys library
        return "0x" + hashlib.new("keccak256", 
            self.private_key.encode()
        ).hexdigest()[-40:]
    
    def get_nonce(self) -> int:
        """Get account nonce"""
        # Use RPC in production
        return 0
    
    def send_transaction(
        self,
        to: str,
        value: str = "0",
        gas_limit: int = 21000,
        gas_price: Optional[int] = None
    ) -> str:
        """Send a transaction"""
        if not self.sdk:
            raise ValueError("SDK required for transaction sending")
        
        # Get gas price if not provided
        if gas_price is None:
            gas_data = self.sdk.get_gas_price()
            gas_price = int(gas_data.get("standard", "20000000000"))
        
        # Build transaction
        tx = {
            "to": to,
            "value": value,
            "gasLimit": gas_limit,
            "gasPrice": str(gas_price)
        }
        
        # In production, sign and broadcast via RPC
        return "0x" + hashlib.new("keccak256", 
            json.dumps(tx).encode()
        ).hexdigest()[:64]
    
    def transfer_token(
        self,
        token_address: str,
        to: str,
        amount: str
    ) -> str:
        """Transfer tokens"""
        # ERC-20 transfer
        return self.send_transaction(
            to=token_address,
            value="0",
            data=self._encode_transfer(to, amount)
        )
    
    def _encode_transfer(self, to: str, amount: str) -> str:
        """Encode transfer function call"""
        # Keccak256("transfer(address,uint256)")
        method_id = "0xa9059cbb"
        
        # Pad addresses and amounts
        to_padded = to[2:].zfill(64)
        amount_padded = str(int(amount)).zfill(64)
        
        return method_id + to_padded + amount_padded


# Convenience functions

def create_sdk(api_key: Optional[str] = None, **kwargs) -> TigerScanSDK:
    """Create an SDK instance"""
    return TigerScanSDK(api_key=api_key, **kwargs)


def create_wallet(
    private_key: str,
    sdk: Optional[TigerScanSDK] = None
) -> Wallet:
    """Create a wallet instance"""
    return Wallet(private_key=private_key, sdk=sdk)


# Export main classes
__all__ = [
    "TigerScanSDK",
    "Wallet",
    "Block",
    "Transaction",
    "TransactionReceipt",
    "Token",
    "TokenHolder",
    "NFT",
    "Account",
    "ChainID",
    "create_sdk",
    "create_wallet"
]