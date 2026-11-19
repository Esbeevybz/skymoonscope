# Bugfix Requirements Document

## Introduction

The pagination utility computes total page count by dividing the total item count by `pageSize`. When `pageSize` is zero (or negative), this division produces `Infinity` (or a nonsensical negative result) instead of raising an error. The fix adds an upfront guard that throws a `RangeError` whenever `pageSize` is not a positive integer, preventing the silent invalid computation.

## Bug Analysis

### Current Behavior (Defect)

1.1 WHEN `pageSize` is zero THEN the system produces `Infinity` as the page count without raising an error  
1.2 WHEN `pageSize` is negative THEN the system produces a negative or nonsensical page count without raising an error

### Expected Behavior (Correct)

2.1 WHEN `pageSize` is zero THEN the system SHALL throw a `RangeError` with the message `'pageSize must be positive'`  
2.2 WHEN `pageSize` is negative THEN the system SHALL throw a `RangeError` with the message `'pageSize must be positive'`

### Unchanged Behavior (Regression Prevention)

3.1 WHEN `pageSize` is a positive integer THEN the system SHALL CONTINUE TO return the correct total page count (`Math.ceil(totalItems / pageSize)`)  
3.2 WHEN `totalItems` is zero and `pageSize` is a positive integer THEN the system SHALL CONTINUE TO return `0` as the page count  
3.3 WHEN `totalItems` is not evenly divisible by `pageSize` THEN the system SHALL CONTINUE TO round up the page count to the next whole number
