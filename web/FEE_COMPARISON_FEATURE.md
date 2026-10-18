# Fee Comparison Feature

## Overview

The Fee Comparison feature allows users to compare transaction fees between Testnet and Mainnet environments in the Sky Moon Scope application. This helps developers understand the cost differences before deploying to production.

## Fee Recommendation Tiers

The application now uses three intuitive fee recommendation tiers:

### Economy Tier
- **Multiplier**: 1.0x
- **Description**: Lowest cost, may take longer to confirm
- **Estimated Time**: ~30-60 seconds
- **Use Case**: Non-urgent transactions where cost is the primary concern

### Standard Tier
- **Multiplier**: 1.5x
- **Description**: Balanced speed and cost for most use cases
- **Estimated Time**: ~15-30 seconds
- **Use Case**: Recommended for most transactions with moderate urgency

### Fast Tier
- **Multiplier**: 2.0x
- **Description**: Priority processing for quick confirmation
- **Estimated Time**: ~5-15 seconds
- **Use Case**: Time-sensitive transactions requiring rapid confirmation

## Implementation

### Backend (stellarRpc.ts)

The fee comparison logic is implemented in `web/lib/stellarRpc.ts`:

- **`fetchFeeComparison()`**: Fetches fee stats from both Testnet and Mainnet RPC endpoints simultaneously using `Promise.allSettled()`
- **Percentage Difference Calculation**: Computes the percentage difference between Mainnet and Testnet fees
- **Error Handling**: Gracefully handles RPC failures by falling back to default fee estimates
- **FeeBumpLevel Enum**: Updated from LOW/MEDIUM/HIGH to ECONOMY/STANDARD/FAST for better user understanding

### Frontend (FeeEstimationPreview.tsx)

The UI component in `web/components/FeeEstimationPreview.tsx`:

- **Toggle Button**: "Compare Networks" button to show/hide the comparison view
- **Side-by-side Cards**: Displays fee estimates for both Testnet and Mainnet
- **Visual Indicators**: 
  - Arrow up/down icons showing which network has higher fees
  - Color-coded cards (cyan for Testnet, emerald for Mainnet)
  - Percentage difference badge
- **Detailed Breakdown**: Shows total fees, economy/standard/fast bump options, and surge multipliers
- **Estimated Time Display**: Each tier now shows estimated confirmation time

### Data Types

Added to `web/lib/sorobantypes.ts`:

```typescript
export interface NetworkFeeComparison {
  testnet: FeeEstimate;
  mainnet: FeeEstimate;
  percentageDifference: number;
  testnetHigher: boolean;
}

export type FeeBumpLevel = 'economy' | 'standard' | 'fast';

export interface FeeBumpOption {
  label: string;
  multiplier: number;
  feeStroops: number;
  feeXlm: string;
  description: string;
  estimatedTime?: string;
}
```

## Usage

1. Run a simulation to generate fee estimates
2. Click the "Compare Networks" button in the Fee Estimation section
3. View the side-by-side comparison of Testnet vs Mainnet fees
4. See the percentage difference and detailed breakdown
5. Choose between Economy, Standard, or Fast tier based on your needs

## Testing

Unit tests are in `web/lib/stellarRpc.feeComparison.test.cjs` and `web/lib/stellarRpc.test.cjs`:

- Tests for correct percentage difference calculation
- Tests for parallel RPC calls
- Tests for RPC failure handling
- Tests for fee bump options between networks
- Tests for surge multiplier preservation
- Tests for new tier system (economy/standard/fast)
- Tests for estimated time display

Run tests with:
```bash
npm test
```

## Example Output

```
Mainnet fees are 0.98% higher

Testnet         Mainnet
─────────────  ─────────────
Total Fee      Total Fee
0.00051 XLM    0.00052 XLM
Economy Bump   Economy Bump
0.00051 XLM    0.00077 XLM
Fast Bump      Fast Bump
0.00102 XLM    0.00154 XLM

Testnet Surge: 1x
Mainnet Surge: 1.2x
```

## Benefits

- **Cost Awareness**: Helps developers understand fee differences before mainnet deployment
- **Informed Decisions**: Enables better cost optimization strategies
- **Risk Mitigation**: Reduces surprise costs when moving from testnet to mainnet
- **Transparency**: Clear visual representation of fee structures
- **User-Friendly Tiers**: Intuitive naming (economy/standard/fast) instead of technical terms
- **Time Estimates**: Users can see expected confirmation times for each tier

## Migration from LOW/MEDIUM/HIGH

The old tier system (LOW/MEDIUM/HIGH) has been replaced with more user-friendly names:

- LOW → Economy (1.0x multiplier)
- MEDIUM → Standard (1.5x multiplier)
- HIGH → Fast (2.0x multiplier)

All existing functionality is preserved, only the naming has changed for better user experience.

## Future Enhancements

- Add historical fee trend charts
- Include more networks (Futurenet, custom RPCs)
- Export comparison data as CSV
- Add fee optimization suggestions based on comparison
- Dynamic time estimates based on current network congestion
