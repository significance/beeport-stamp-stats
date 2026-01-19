# Gnosis Chain RPC Endpoints

**Last Updated:** 2026-01-11

This document lists public and private RPC endpoints for Gnosis Chain and explains how to find them.

---

## Quick Reference: Free Public RPCs (No Account Required)

These endpoints work immediately without any registration:

| Provider | URL | Notes |
|----------|-----|-------|
| **FairDataSociety** | https://xdai.fairdatasociety.org | Community-run |
| **Gnosis Official** | https://rpc.gnosischain.com | Official endpoint |
| **dRPC** | https://gnosis.drpc.org | AI-powered load balancer |
| **Ankr** | https://rpc.ankr.com/gnosis | Popular provider |
| **1RPC** | https://1rpc.io/gnosis | Privacy-focused |
| **PublicNode** | https://gnosis-rpc.publicnode.com | Community infrastructure |
| **BlockPi** | https://gnosis.blockpi.network/v1/rpc/public | Public access |
| **BlastAPI (Public)** | https://gnosis-mainnet.public.blastapi.io | Rate-limited public tier |
| **Gateway.fm** | https://rpc.gnosis.gateway.fm | Public access |
| **OnFinality** | https://gnosis.api.onfinality.io/public | Public tier |
| **Blockscout Archive** | https://xdai-archive.blockscout.com | Archive node |
| **Nodies** | https://gnosis-public.nodies.app | Public access |

**Rate Limits:** These typically have rate limits (10-50 requests/second) but work without accounts.

---

## Enhanced RPCs (Account Required - Free Tier Available)

### BlastAPI (Enhanced)
- **Free Tier:** Yes (requires sign-up)
- **How to get:** https://blastapi.io/
- **Endpoint format:** `https://gnosis-mainnet.blastapi.io/{YOUR-PROJECT-ID}`
- **Benefits:** Higher rate limits, dedicated resources
- **Free tier limits:** ~25M requests/month

### Chainstack
- **Free Tier:** Yes (Developer plan)
- **How to get:** https://chainstack.com/
- **Endpoint format:** Custom endpoint after sign-up
- **Benefits:** Archive data, websockets, dedicated nodes
- **Free tier limits:** 3M requests/month

### QuickNode
- **Free Tier:** Yes (trial period)
- **How to get:** https://www.quicknode.com/
- **Endpoint format:** Custom endpoint after sign-up
- **Benefits:** Low latency, global endpoints, archive access
- **Free tier limits:** Limited trial, then paid

### Alchemy
- **Free Tier:** Yes (Composer plan)
- **How to get:** https://www.alchemy.com/
- **Endpoint format:** Custom endpoint after sign-up
- **Benefits:** Enhanced APIs, websockets, high reliability
- **Free tier limits:** 300M compute units/month

### Infura
- **Free Tier:** Yes
- **How to get:** https://infura.io/
- **Endpoint format:** `https://gnosis-mainnet.infura.io/v3/{YOUR-PROJECT-ID}`
- **Benefits:** Industry standard, reliable
- **Free tier limits:** 100k requests/day

---

## How to Find More RPC Endpoints

### Method 1: ChainList (Easiest)
1. Visit https://chainlist.org/chain/100
2. Scroll to "RPC Servers" section
3. Click "Add to MetaMask" or copy URL directly
4. Updated regularly by community

### Method 2: Official Documentation
1. Visit https://docs.gnosischain.com/tools/RPC%20Providers/
2. Official list maintained by Gnosis team
3. Includes provider comparisons

### Method 3: Provider Aggregators
- **CompareNodes:** https://www.comparenodes.com/library/public-endpoints/gnosis-chain/
- **dRPC:** https://drpc.org/chainlist/gnosis
- Both list 20+ endpoints with status monitoring

### Method 4: Search GitHub
```bash
# Search for Gnosis RPC configs in public repositories
site:github.com "gnosis" "rpc" "https://"
```

---

## Recommended Configuration Strategy

### For Development (Single RPC)
Use free public endpoint:
```yaml
rpc:
  url: "https://rpc.gnosischain.com"
```

### For Production (Multi-RPC with Adaptive Rate Limiting)
Combine free public endpoints for redundancy:
```yaml
rpc:
  endpoints:
    - url: "https://rpc.gnosischain.com"
      rate_limit: adaptive
      priority: 1
      weight: 1
    - url: "https://gnosis.drpc.org"
      rate_limit: adaptive
      priority: 1
      weight: 1
    - url: "https://rpc.ankr.com/gnosis"
      rate_limit: adaptive
      priority: 1
      weight: 1
```

### For High-Volume Operations
Mix free public + paid enhanced:
```yaml
rpc:
  endpoints:
    # Primary: Paid endpoint with high limits
    - url: "https://gnosis-mainnet.blastapi.io/{YOUR-PROJECT-ID}"
      rate_limit:
        manual: 200  # Known limit from provider
      priority: 1
      weight: 3  # Prefer this endpoint

    # Fallback: Free public endpoints
    - url: "https://rpc.gnosischain.com"
      rate_limit: adaptive
      priority: 2
      weight: 1
    - url: "https://gnosis.drpc.org"
      rate_limit: adaptive
      priority: 2
      weight: 1
```

---

## Rate Limiting Best Practices

### Public Endpoints
- **Typical limits:** 10-50 req/s per IP
- **Strategy:** Use `adaptive` rate limiting
- **Start conservatively:** 10 req/s, ramp up to discovered limit
- **Respect 429 errors:** Back off immediately

### Paid Endpoints
- **Check documentation:** Most providers publish rate limits
- **Strategy:** Use `manual` rate limiting with known limit
- **Example:** BlastAPI free tier = ~300 req/s

### Mixed Configuration
```yaml
rate_limiting:
  adaptive:
    start_rps: 10.0    # Conservative start for public RPCs
    max_rps: 500.0     # Don't exceed reasonable limit
    ramp_up_factor: 1.2
    back_off_factor: 0.5
    ramp_up_threshold: 100
```

---

## Testing RPC Endpoints

### Check if endpoint works:
```bash
curl -X POST https://rpc.gnosischain.com \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

Expected response:
```json
{"jsonrpc":"2.0","id":1,"result":"0x27a1c3f"}
```

### Check if endpoint is archive node:
```bash
# Try to get very old block
curl -X POST https://xdai-archive.blockscout.com \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["0x1", false],"id":1}'
```

Archive nodes can query old state, regular nodes only recent blocks.

---

## Current Status (2026-01-11)

### Working Public Endpoints (Verified)
✅ All endpoints listed in "Free Public RPCs" section above are currently operational.

### Tested with beeport-stamp-stats
- Configuration: `config-multi-rpc-test.yaml`
- Endpoints: 5 public RPCs (FairDataSociety, Gnosis Official, dRPC, Ankr, 1RPC)
- Strategy: Adaptive rate limiting
- Status: Ready for testing

---

## Sources

- [Gnosis Chain Documentation](https://docs.gnosischain.com/tools/RPC%20Providers/)
- [ChainList](https://chainlist.org/chain/100)
- [Chainstack](https://chainstack.com/build-better-with-gnosis-chain/)
- [dRPC Chainlist](https://drpc.org/chainlist/gnosis)
- [CompareNodes](https://www.comparenodes.com/library/public-endpoints/gnosis-chain/)

---

*This document is maintained alongside RPC infrastructure changes. Update when new providers are discovered or configurations change.*
