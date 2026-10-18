# Syntax Highlighter: Custom Tokenizer Optimization

## Overview
The `SyntaxHighlighter` component uses a custom, lightweight regex-based tokenizer instead of importing external syntax highlighting libraries like Prism or highlight.js. This keeps the bundle small while maintaining full control over styling and supported languages.

## Why Custom Tokenization?
- **Zero external dependencies**: No Prism, highlight.js, or similar libraries bloating the bundle
- **Selective language support**: Only loads tokenization rules for languages actually needed (Rust contracts and XDR)
- **CSS-driven styling**: Uses Tailwind classes for theming, no embedded styles
- **Memoization**: Results are cached via `useMemo` to prevent unnecessary re-parsing

## Supported Languages

### Contract (Rust-like syntax)
- Keywords: `pub`, `fn`, `let`, `mut`, `const`, `if`, `else`, `for`, `while`, `loop`, `match`, `return`, `true`, `false`, `Self`, `self`, `struct`, `enum`, `impl`, `use`, `mod`, `type`, `trait`, `where`, `as`, `in`, `ref`, `move`, `async`, `await`, `unsafe`
- Types: `u32`, `u64`, `u128`, `i32`, `i64`, `i128`, `bool`, `String`, `Address`, `Symbol`, `Bytes`, `BytesN`, `Vec`, `Map`, `Option`, `Result`, `SorobanType`, `IntoVal`, `TryFromVal`, `Env`, `BigInt`, `Duration`, `Timepoint`
- Token types: strings, comments (line/block), numbers (hex/float/int), keywords, types, functions, operators, identifiers

### XDR (Soroban XDR format)
- Hex strings and base64 payloads
- Field names and XDR keywords: `enum`, `struct`, `union`, `case`, `default`, `void`, `bool`, `int`, `unsigned`, `hyper`, `opaque`, `string`, `array`, `optional`, `switch`, `typedef`, `const`
- Token types: hex, base64, strings, numbers, fields, keywords, punctuation, identifiers

## Usage

```tsx
import { SyntaxHighlighter } from '@/components/SyntaxHighlighter';

// Highlight contract source code
<SyntaxHighlighter 
  code={contractCode}
  language="contract"
  showLineNumbers={true}
  maxHeight="400px"
/>

// Highlight XDR output
<SyntaxHighlighter 
  code={xdrOutput}
  language="xdr"
  maxHeight="300px"
/>
```

## Implementation Details

### Tokenization Process
1. **Token Rules**: Each language has an ordered array of regex patterns with sticky flags
2. **Regex Matching**: Code is scanned left-to-right, matching against rules in order
3. **Token Classification**: Each match is tagged with a token type (keyword, string, comment, etc.)
4. **Line Splitting**: Tokens containing newlines are split into separate line arrays
5. **Rendering**: Each token is wrapped in a `<span>` with a Tailwind class corresponding to its type

### Color Map (Tailwind Classes)
| Token Type | Color | Class |
|------------|-------|-------|
| keyword | cyan | `text-cyan-400` |
| type | yellow | `text-yellow-400` |
| string | emerald | `text-emerald-400` |
| number | orange | `text-orange-400` |
| comment | slate | `text-slate-500 italic` |
| function | violet | `text-violet-400` |
| operator | slate | `text-slate-300` |
| field (XDR) | blue | `text-blue-400` |
| hex (XDR) | fuchsia | `text-fuchsia-400` |
| base64 (XDR) | amber | `text-amber-300/80` |

## Performance Considerations
- **Memoization**: Tokenization is only re-run when `code` or `language` props change
- **Sticky Regex**: Uses `lastIndex` and the `y` (sticky) flag for efficient sequential matching
- **No DOM Thrashing**: Line grouping and token rendering happen in JS, then rendered in a single table

## Future Enhancements
- Add support for additional Soroban contract languages (e.g., Stellar SDK TypeScript)
- Expand XDR token rules for deeper structural analysis
- Add optional syntax error detection and highlighting

## References
- Component: `/web/components/SyntaxHighlighter.tsx`
- Used in: Result viewer, contract inspection panels, XDR display
