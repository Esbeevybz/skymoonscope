# SkyMoonScope - Monorepo Architecture

## System Overview

SkyMoonScope is a multi-layered decentralized finance platform built on Soroban. The monorepo consists of four primary layers:

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Web Frontend Layer                          │
│  (React/Next.js - User Interface & Wallet Integration)             │
│  ├── Components                                                     │
│  ├── Pages & Routing                                                │
│  ├── Context & State Management                                     │
│  └── Integration with Soroban Contracts                             │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Soroscope Indexing Layer                         │
│  (Off-Chain Indexer & Data Aggregator)                             │
│  ├── Event Indexing from Soroban                                    │
│  ├── Historical Data Aggregation                                    │
│  ├── Query API for Frontend                                         │
│  └── Real-Time Event Streaming                                      │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Core Protocol Layer                              │
│  (Rust - Soroban Smart Contract Backends)                          │
│  ├── Liquidity Pools & AMM                                          │
│  ├── Token Management                                               │
│  ├── Factory Contracts                                              │
│  ├── Fee Market                                                     │
│  ├── Cross-Chain Verification                                       │
│  └── Emergency Guard (Pause Control)                                │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│                  Soroban Smart Contracts Layer                      │
│  (Individual Contract Implementations)                              │
│  ├── Auctions (English, Dutch)                                      │
│  ├── Asset Transfers & Bridges                                      │
│  ├── Collateralized Debt Positions (CDP)                            │
│  ├── Concentrated AMM                                               │
│  ├── Cross-Chain Payloads                                           │
│  ├── DID Registry                                                   │
│  ├── Error Codes & Utilities                                        │
│  └── [20+ Additional Contracts]                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Detailed Layer Architecture

### Layer 1: Web Frontend (Next.js)
**Location:** `/web`

Provides the user interface for interacting with the protocol:
- **Components:** Reusable React components for UI elements
- **Pages:** Route handlers and page logic
- **Context:** Global state management for wallet connection, token data, user preferences
- **Hooks:** Custom React hooks for contract interactions
- **Integration:** Direct communication with Soroban via Soroban SDK

### Layer 2: Soroscope Indexer (Off-Chain)
**Location:** `/soroscope`

Indexes and aggregates on-chain data for efficient querying:
- **Event Indexing:** Tracks contract events emitted by smart contracts
- **Data Aggregation:** Builds indexes for pools, tokens, transactions
- **Query API:** Provides REST/GraphQL endpoints for frontend queries
- **Streaming:** Real-time event notifications for live updates
- Mirrors the same four-layer structure internally

### Layer 3: Core Protocol (Rust/Soroban)
**Location:** `/core`

Central protocol logic written in Rust for Soroban:
- **Fee Market:** Dynamic fee calculation and market mechanisms
- **AMM & Pools:** Liquidity pool management and swaps
- **Cross-Chain Support:** Bridge verification and payload handling
- **Storage Optimization:** Efficient data structures and caching
- **Utilities:** Common functions shared across contracts

### Layer 4: Smart Contracts (Soroban WASM)
**Location:** `/contracts`

Individual contract implementations, organized by function:

#### Marketplace & Trading
- `auction_factory` - Factory for deploying auction instances
- `english_auction` - Traditional ascending price auction
- `dutch_auction` - Descending price auction

#### Asset Management
- `batch_transfer` - Multi-asset transfers
- `token` - Token implementation
- `token_guard` - Token access control

#### DeFi Primitives
- `concentrated_amm` - Concentrated liquidity AMM
- `liquidity_pool` - Traditional AMM pool
- `factory` - Pool factory contract
- `flash_loan` - Flash lending mechanism

#### Financial Instruments
- `cdp` - Collateralized Debt Positions
- `dutch_auction` - Dutch auction variant

#### Cross-Chain & Bridges
- `cross_call` - Cross-contract communication
- `cross_chain_payload` - Payload verification for bridges
- `cross_chain_verifier` - Signature verification for cross-chain ops

#### Governance & Security
- `emergency_guard` - Pause/emergency control mechanism
- `did_registry` - Decentralized Identity
- `error_codes` - Unified error definitions

#### Testing & Examples
- `hello_soroban` - Basic example contract
- `cpu_heavy` - Performance testing
- `crucible-example-gasless` - Gasless transaction example

## EmergencyGuard - Architecture & Design Summary

### System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    All Soroban Contracts                        │
│  (liquidity_pool, token, factory, cross_call, etc.)            │
└──────────────────────────┬──────────────────────────────────────┘
                           │ depends on
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                 EmergencyGuard Crate                            │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ DefaultEmergencyGuard (Main Implementation)              │  │
│  │  • init_guard()                                          │  │
│  │  • check_not_paused()                                    │  │
│  │  • set_pause_state() - granular pause control            │  │
│  │  • emergency_pause_all() / resume_all()                  │  │
│  │  • rotate_admin() - secure admin transitions             │  │
│  │  • add_admin() / remove_admin()                          │  │
│  │  • get_admins() / get_threshold()                        │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ PauseType (Bitmask - 32 operations in u32)               │  │
│  │  • SWAP        = 1 << 0                                  │  │
│  │  • DEPOSIT     = 1 << 1                                  │  │
│  │  • WITHDRAW    = 1 << 2                                  │  │
│  │  • TRANSFER    = 1 << 3                                  │  │
│  │  • MINT        = 1 << 4                                  │  │
│  │  • BURN        = 1 << 5                                  │  │
│  │  (plus 26 more available)                                │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ Storage (Instance Storage via DataKey)                   │  │
│  │  • PauseState       -> u32 (bitmask)                    │  │
│  │  • Admins           -> Vec<Address>                      │  │
│  │  • SignatureThreshold -> u32                             │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ Error Types                                              │  │
│  │  • Unauthorized                                          │  │
│  │  • Paused                                                │  │
│  │  • InsufficientSignatures                                │  │
│  │  • InvalidThreshold                                      │  │
│  │  • AdminNotFound                                         │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Data Flow Architecture

### User Request to Smart Contract Execution

```
Web Frontend (Layer 1)
    │ User Action (swap, deposit, etc.)
    ├─ Fetch data from Soroscope API
    ├─ Build contract invocation
    └─ Submit transaction to Soroban
         │
         ▼
Soroban Network
    │ Transaction Validation
    ├─ Execute SmartContract Logic (Layer 4)
    │   └─ Call Core Protocol (Layer 3)
    │       └─ Validate with EmergencyGuard
    └─ Emit Events
         │
         ▼
Soroscope Indexer (Layer 2)
    │ Listens to on-chain events
    ├─ Indexes transactions & state
    ├─ Aggregates data
    └─ Exposes via Query API
         │
         ▼
Web Frontend (Layer 1)
    │ Real-time UI updates
    └─ Displays results to user
```

### Contract Initialization Pattern

```
Contract Initialization
       ▼
┌──────────────────────────┐
│ pub fn initialize(...) { │
│   ...                    │
│   init_guard(admins, 1)  │────► Initialize guard with admin list
│ }                        │
└──────────────────────────┘
       ▲
       │ (once per contract)

Operation Execution
       ▼
┌────────────────────────────────────┐
│ pub fn swap(amount) {              │
│   check_not_paused(SWAP)?  ◄────── Check if operation paused
│   // Execute swap                  │
│   transfer_tokens(...)             │
│ }                                  │
└────────────────────────────────────┘
       ▲
       │ (every operation)

Admin Control
       ▼
┌──────────────────────────────────┐
│ pub fn pause_swaps() {            │
│   set_pause_state(SWAP, true)     │ Pause swaps, keep others active
│ }                                 │
└──────────────────────────────────┘
       ▲
       │ (admin only, can affect all contracts)

Emergency Control
       ▼
┌──────────────────────────────────┐
│ pub fn emergency_pause_all() {    │
│   emergency_pause_all()  ◄─ Pause all (bitmask = U32::MAX)
│ }                                 │
└──────────────────────────────────┘
```

## Contract Dependencies

### Direct Dependencies

```
Liquidity Pool Contract
├─ Depends on: Core Protocol (AMM logic)
├─ Depends on: EmergencyGuard (pause control)
└─ Depends on: Token Contract (asset transfers)

Token Contract
├─ Depends on: EmergencyGuard (pause control)
└─ Interacts with: Liquidity Pool (swaps)

Factory Contract
├─ Depends on: Liquidity Pool (pool creation)
├─ Depends on: Token Contract (token management)
└─ Depends on: EmergencyGuard (pause control)

Cross-Chain Verifier
├─ Depends on: Core Protocol (verification logic)
├─ Depends on: Cross-Chain Payload
└─ Depends on: EmergencyGuard (pause control)

Fee Market
├─ Depends on: Core Protocol
├─ Depends on: Liquidity Pool (fee application)
└─ Depends on: EmergencyGuard (pause control)
```

## Data Structure

### PauseState Bitmask Example

```
Initial State (Nothing Paused):
  0b00000000000000000000000000000000 = 0x00000000

Pause Swaps:
  0b00000000000000000000000000000001 = 0x00000001 (SWAP bit set)

Pause Swaps + Deposits:
  0b00000000000000000000000000000011 = 0x00000003 (SWAP + DEPOSIT bits)

Pause Swaps + Deposits + Withdrawals:
  0b00000000000000000000000000000111 = 0x00000007 (SWAP + DEPOSIT + WITHDRAW)

Emergency Pause All:
  0b11111111111111111111111111111111 = 0xFFFFFFFF (All bits set)
```

## Admin Rotation Flow

```
Current State:
  Admins: [admin1, admin2, admin3]

admin1 initiates rotation to admin4:
  admin1.require_auth() ✓

After rotation:
  Admins: [admin4, admin2, admin3]

Result:
  • admin1 is no longer an admin
  • admin4 is now an admin
  • No funds transferred
  • Change takes effect immediately
```

## Multi-Signature (Current vs Future)

### Current Implementation

```
Admin List: [admin1, admin2, admin3]
Threshold: 1

Any single admin can:
  ✓ set_pause_state()
  ✓ emergency_pause_all()
  ✓ rotate_admin()
```

### Future Enhancement

```
Admin List: [admin1, admin2, admin3]
Threshold: 2

For critical operations, require 2 of 3 signatures:
  require verify_signature(admin1, sig1) ✓
  require verify_signature(admin2, sig2) ✓

  Then: pause_all() executes
```

## Storage Efficiency

### Before (Boolean per contract)

```
liquidity_pool:  bool paused (1 byte per LP contract) + logic + tests
token:           bool paused (1 byte per token contract) + logic + tests
factory:         bool paused (1 byte per factory) + logic + tests
...multiply across 100+ contracts...
```

### After (Unified Bitmask)

```
ALL contracts: u32 pause_state (4 bytes total, 32 operations)
              Vec<Address> admins (shared)
              u32 threshold (shared)

Benefits:
  • 87.5% smaller pause state (1 byte vs 8+ bytes per operation)
  • Single implementation (no duplication)
  • 26 unused bits available for future operations
```

## Integration Checklist by Contract

| Contract       | Current Status        | Todo                     |
| -------------- | --------------------- | ------------------------ |
| liquidity_pool | EmergencyGuard bitmask (`PauseState` u32) | Done |
| token          | No pause              | Add pause support        |
| factory        | No pause              | Add pause support        |
| cross_call     | No pause              | Add pause support        |
| hello_soroban  | N/A                   | N/A                      |
| cpu_heavy      | N/A                   | N/A                      |
| storage_heavy  | N/A                   | N/A                      |

## Error Handling Examples

```rust
// Check if operation is paused
match DefaultEmergencyGuard::check_not_paused(&env, PauseType::SWAP) {
    Ok(()) => {
        // Continue with operation
    },
    Err(GuardError::Paused) => {
        // Operation blocked - already logged by guard
        return Err(ContractError::Paused);
    },
    Err(_) => {
        // Other errors (initialization issues, etc.)
        return Err(ContractError::InternalError);
    }
}
```

## Event Audit Trail

All administrative actions are logged:

```
[TIMESTAMP] Init Guard: admins=[addr1, addr2], threshold=2
[TIMESTAMP] Pause state updated: operation=1, paused=true
[TIMESTAMP] Emergency pause all activated by addr1
[TIMESTAMP] Resume all activated by addr2
[TIMESTAMP] Admin added: addr3
[TIMESTAMP] Admin removed: addr4
[TIMESTAMP] Admin rotated: addr1 -> addr5
```

## Monorepo Structure

### Top-Level Organization

```
skymoonscope/
├── Cargo.toml                          ◄── Workspace root (contracts only)
├── Cargo.lock
│
├── web/                                ◄── Layer 1: Frontend
│   ├── package.json
│   ├── next.config.js
│   ├── tsconfig.json
│   ├── components/                     ◄── React components
│   ├── pages/                          ◄── Route handlers
│   ├── context/                        ◄── Global state
│   ├── hooks/                          ◄── Custom React hooks
│   ├── lib/                            ◄── Utilities
│   └── styles/                         ◄── CSS/Tailwind
│
├── soroscope/                          ◄── Layer 2: Indexer (Copy of repo structure)
│   ├── Cargo.toml                      ◄── Full Rust project
│   ├── core/                           ◄── Indexing logic
│   ├── contracts/                      ◄── Any contract refs
│   ├── web/                            ◄── API server (optional)
│   └── docs/                           ◄── Indexer docs
│
├── core/                               ◄── Layer 3: Core Protocol
│   ├── Cargo.toml                      ◄── Rust project
│   ├── src/                            ◄── Protocol implementations
│   │   ├── lib.rs                      ◄── Public API
│   │   ├── fee_market/                 ◄── Fee mechanisms
│   │   ├── amm/                        ◄── AMM logic
│   │   ├── crosschain/                 ◄── Bridge logic
│   │   └── utils/                      ◄── Utilities
│   ├── tests/                          ◄── Integration tests
│   ├── benches/                        ◄── Performance benchmarks
│   └── examples/                       ◄── Usage examples
│
├── contracts/                          ◄── Layer 4: Smart Contracts
│   ├── Cargo.toml                      ◄── Workspace manifest
│   │
│   ├── emergency_guard/                ◄── Pause/Control Mechanism
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs                  ◄── Core implementation
│   │   │   └── test.rs                 ◄── Unit tests
│   │   ├── examples/
│   │   │   └── simple_token.rs         ◄── Integration example
│   │   └── README.md
│   │
│   ├── liquidity_pool/                 ◄── AMM Pool
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── README.md
│   │
│   ├── token/                          ◄── Token Implementation
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── README.md
│   │
│   ├── factory/                        ◄── Pool Factory
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── README.md
│   │
│   ├── cross_chain_verifier/           ◄── Cross-Chain Verification
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   ├── README.md
│   │   ├── PERFORMANCE_OPTIMIZATION.md
│   │   └── SIGNATURE_VERIFICATION.md
│   │
│   ├── [20+ Additional Contracts]
│   │   ├── auction_factory/
│   │   ├── english_auction/
│   │   ├── dutch_auction/
│   │   ├── batch_transfer/
│   │   ├── concentrated_amm/
│   │   ├── cdp/
│   │   ├── cross_chain_payload/
│   │   ├── cross_call/
│   │   ├── did_registry/
│   │   ├── flash_loan/
│   │   ├── token_guard/
│   │   ├── error_codes/
│   │   ├── hello_soroban/
│   │   ├── cpu_heavy/
│   │   └── ...
│   │
│   └── EMERGENCY_GUARD_INTEGRATION.md  ◄── Integration guide
│
├── docs/                               ◄── Documentation
│   ├── ARCHITECTURE.md                 ◄── This file
│   ├── development.md                  ◄── Development guide
│   ├── deployment.md                   ◄── Deployment procedures
│   ├── api_documentation.md            ◄── API reference
│   ├── IMPLEMENTATION_GUIDE.md         ◄── Feature guides
│   └── [30+ Additional Docs]
│
├── scripts/                            ◄── Build/Deploy Scripts
│   └── ...
│
├── tests/                              ◄── Integration Tests
│   └── ...
│
├── README.md                           ◄── Project overview
├── Dockerfile                          ◄── Docker setup
└── docker-compose.yml                  ◄── Local environment
```

## Key Features Summary

| Feature               | Description                   | Benefit                               |
| --------------------- | ----------------------------- | ------------------------------------- |
| **Granular Pausing**  | 32 individual operation types | Pause swaps, keep withdrawals working |
| **Multi-Sig Ready**   | Threshold + admin list        | Scale to N-of-M governance            |
| **Admin Rotation**    | Direct replacement            | No fund movement needed               |
| **Efficient Storage** | Bitmask (4 bytes)             | vs 8+ bytes traditional boolean       |
| **Event Logging**     | All actions logged            | Complete audit trail                  |
| **Error Types**       | Specific GuardError codes     | Clear error handling                  |
| **Test Coverage**     | Comprehensive unit tests      | Verify all operations work            |
| **Documentation**     | 3 detailed guides             | Easy onboarding                       |

## Deployment Timeline

```
Week 1: Complete ✅
  • Design & implement EmergencyGuard
  • Write tests and documentation
  • Create examples

Week 2: Integration (In Progress)
  • Integrate into liquidity_pool
  • Integrate into token
  • Integrate into factory

Week 3: Testing & Review
  • Test on testnet
  • Gather feedback
  • Optimize if needed

Week 4: Production
  • Merge to main
  • Deploy to mainnet
  • Monitor and support
```

## Security Guarantees

1. **Admin Authorization** - Only authorized admins can pause/unpause
2. **Atomic Operations** - Pause state changes are atomic
3. **No Fund Movement** - Admin rotation doesn't move funds
4. **Threshold Protection** - Can't remove below minimum admins
5. **Event Logging** - All operations logged for audit
6. **Graceful Degradation** - Pause doesn't lose user funds

## Module Responsibilities & Boundaries

### Web Frontend (`/web`) - Layer 1

**Responsibilities:**
- User interface for pool management, trading, and governance
- Wallet connection and transaction signing
- Real-time data visualization
- User state persistence

**Key Technologies:**
- Next.js 13+ (React)
- TypeScript
- Tailwind CSS
- Soroban SDK for contract interaction
- Wallet integrations (Freighter, etc.)

**Interface:**
- Consumes: Soroscope Query API, Soroban contracts
- Produces: User transactions, blockchain queries

---

### Soroscope Indexer (`/soroscope`) - Layer 2

**Responsibilities:**
- Ingests events from Soroban blockchain
- Maintains historical indexes for fast querying
- Serves aggregated data to frontend
- Real-time event streaming

**Key Components:**
- Event listeners (Soroban node RPC)
- Database/Index storage
- Query API (GraphQL or REST)
- Event streaming (WebSocket)

**Interface:**
- Consumes: Soroban blockchain events
- Produces: Query API endpoints, WebSocket streams

---

### Core Protocol (`/core`) - Layer 3

**Responsibilities:**
- Shared protocol logic for smart contracts
- Fee market mechanisms
- AMM calculations
- Cross-chain verification logic
- Performance optimizations

**Key Features:**
- Re-usable library for contracts
- Benchmarking and optimization
- Documentation of protocol semantics
- Example implementations

**Interface:**
- Consumed by: Smart contracts (as dependency)
- Produces: WASM libraries, type definitions

---

### Smart Contracts (`/contracts`) - Layer 4

**Responsibilities:**
- Individual contract business logic
- On-chain state management
- Transaction validation and execution
- Event emission

**Organization by Function:**

| Category | Contracts | Purpose |
|----------|-----------|---------|
| **AMM & Liquidity** | liquidity_pool, factory, concentrated_amm | Pool management and swaps |
| **Token Management** | token, token_guard, batch_transfer | Asset handling |
| **Trading** | english_auction, dutch_auction, auction_factory | Auction mechanisms |
| **Finance** | cdp, flash_loan | Financial primitives |
| **Cross-Chain** | cross_chain_verifier, cross_chain_payload, cross_call | Bridge support |
| **Governance** | emergency_guard, did_registry | Control & identity |
| **Utilities** | error_codes, hello_soroban | Shared utilities |
| **Testing** | cpu_heavy | Performance testing |

---

## Data Dependencies

### Contract to Contract Communication

```
User Transaction
    │
    ├─ Liquidity Pool Contract
    │   ├─ Calls: Token Contract (for transfers)
    │   ├─ Calls: EmergencyGuard (for pause checks)
    │   ├─ Uses: Core Protocol (for AMM logic)
    │   └─ Emits: Events (indexed by Soroscope)
    │
    ├─ Cross-Chain Verifier
    │   ├─ Verifies: Signatures using core logic
    │   ├─ Calls: Cross-Chain Payload contract
    │   └─ Emits: Verification events
    │
    └─ Factory Contract
        ├─ Creates: New Pool instances
        ├─ Calls: Pool + Token contracts
        └─ Emits: Creation events
```

### Data Flow Timeline

```
T=0: User initiates swap on web frontend
T=1: Frontend fetches current pool state from Soroscope API
T=2: Frontend estimates output using Core protocol logic
T=3: Frontend submits signed transaction to Soroban
T=4: Soroban executes Liquidity Pool contract
T=5: LP contract checks EmergencyGuard (not paused)
T=6: LP contract calls Token contract for transfers
T=7: LP contract emits events with execution details
T=8: Soroscope indexer detects and processes events
T=9: Soroscope indexes transaction and pool state changes
T=10: Frontend polls Soroscope API and receives updates
T=11: Frontend updates UI with new balances and rates
```

---

## Integration Patterns

### Adding a New Contract to the Monorepo

1. **Create Contract Directory**
   ```bash
   mkdir contracts/my_contract
   cd contracts/my_contract
   cargo init --name my_contract
   ```

2. **Add Dependencies**
   - Add to root `Cargo.toml` workspace members
   - Import core protocol as dependency: `skymoonscope_core = { path = "../core" }`
   - Import emergency_guard if pause control needed

3. **Implement Contract**
   ```rust
   use soroban_sdk::*;
   use emergency_guard::DefaultEmergencyGuard;
   use skymoonscope_core::*;
   
   #[contract]
   pub struct MyContract;
   
   #[contractimpl]
   impl MyContract {
       pub fn initialize(env: Env, admins: Vec<Address>) {
           DefaultEmergencyGuard::init_guard(&env, admins, 1);
       }
       
       pub fn execute(env: Env) {
           // Check if operation is paused
           DefaultEmergencyGuard::check_not_paused(&env, PauseType::CUSTOM)?;
           // Business logic...
       }
   }
   ```

4. **Add Tests**
   - Unit tests in `src/test.rs`
   - Integration tests in `/tests` directory

5. **Document**
   - Add `README.md` with API reference
   - Update `/docs/ARCHITECTURE.md`
   - Add examples to `/contracts/my_contract/examples`

6. **Build & Deploy**
   ```bash
   soroban contract build --manifest-path contracts/my_contract/Cargo.toml
   ```

### Updating the Frontend for a New Contract

1. **Generate TypeScript Bindings**
   ```bash
   soroban contract ts-bindings contracts/my_contract/target/wasm32-unknown-unknown/release/my_contract.wasm
   ```

2. **Create React Hook**
   ```typescript
   // web/hooks/useMyContract.ts
   import { useContract } from './useContract';
   export function useMyContract() {
       return useContract('my_contract');
   }
   ```

3. **Create Component**
   ```tsx
   // web/components/MyFeature.tsx
   import { useMyContract } from '../hooks/useMyContract';
   export function MyFeature() {
       const contract = useMyContract();
       // Implementation...
   }
   ```

4. **Add to Soroscope Indexer**
   - Subscribe to contract events in indexer
   - Create queries in API layer
   - Update frontend query hooks

---

## Common Workflows

### Emergency Pause Procedure

```
1. Admin detects security issue
   └─> Calls emergency_guard::emergency_pause_all()

2. All contracts check pause state
   └─> New transactions fail with Paused error

3. Soroscope indexes pause event
   └─> Frontend displays "System Paused" message

4. Investigation period (admin controlled)
   └─> Team investigates root cause

5. Fix deployed
   └─> Admins call emergency_guard::resume_all()

6. System resumes operations
   └─> Frontend resumes normal operation
```

### Adding a New Token

```
1. Deploy Token Contract
   └─> Specify mint account, initial supply

2. Create Liquidity Pool via Factory
   └─> Factory deploys new Pool contract
   └─> Links token + stablecoin pair

3. Soroscope indexes new pool
   └─> Adds pool to query indexes

4. Frontend detects new pool
   └─> Fetches pool stats from Soroscope
   └─> Displays trading pair to users
```

### Cross-Chain Verification Flow

```
1. Bridge detects event on external chain
   └─> Creates cross-chain payload

2. Cross-Chain Payload contract stores it
   └─> Available for verification

3. Cross-Chain Verifier validates signatures
   └─> Checks quorum and threshold
   └─> Emits verification event

4. Dependent contracts act on verification
   └─> Release bridged assets
   └─> Update state accordingly

5. Soroscope indexes verification
   └─> Tracks cross-chain transaction
```

---

## Future Enhancements

- [ ] Timelock for emergency_pause_all (e.g., 1 hour delay)
- [ ] Voting system for admin decisions
- [ ] Pause duration limits (auto-unpause)
- [ ] Cross-contract guard coordination
- [ ] Dashboard for monitoring pause states
- [ ] Integration with Stellar's multi-sig accounts
