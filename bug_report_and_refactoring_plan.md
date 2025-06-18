# Bug Report and Refactoring Plan for nvo_servers

## Summary
This document outlines bugs found in the nvo_servers HTTP server library and proposes refactoring to improve code quality, safety, and adherence to functional programming principles.

## Critical Bugs Found

### 1. **BSD/macOS Event Processing Bug** (High Priority)
**Location**: `src/http/async_bsd_http_server.rs:39`
**Issue**: The kqueue implementation only processes one event at a time
```rust
let events_number = unsafe { kqueue_sys::kevent(kqueue, core::ptr::null(), 0, &mut kevent, 1, core::ptr::null()) };
```
**Impact**: Severe performance degradation under load, potential request starvation
**Fix**: Process multiple events in a single syscall

### 2. **Request Size Limitation** (High Priority)
**Location**: `src/http/async_handler.rs:57`
**Issue**: Fixed 8192 byte buffer for reading HTTP requests
```rust
let mut buf = [0u8; 8192];
```
**Impact**: Cannot handle requests with headers larger than 8KB
**Fix**: Implement dynamic buffer sizing or streaming parser

### 3. **Resource Leak on Error** (Medium Priority)
**Location**: Multiple locations in Linux/BSD implementations
**Issue**: File descriptors not removed from epoll/kqueue on connection errors
**Impact**: Memory leak, eventual resource exhaustion
**Fix**: Ensure cleanup in all error paths

### 4. **Missing Error Context** (Medium Priority)
**Location**: Throughout the codebase
**Issue**: Errors logged without sufficient context (e.g., which connection failed)
**Impact**: Difficult debugging in production
**Fix**: Add connection identifiers to error messages

### 5. **Race Condition in Connection State** (Medium Priority)
**Location**: Connection state transitions
**Issue**: Connection can be in inconsistent state if worker fails between remove and re-insert
**Impact**: Lost connections, undefined behavior
**Fix**: Use atomic state transitions or better locking strategy

## Code Quality Issues

### 1. **Violation of Functional Programming Principles**
- Excessive use of loops instead of iterators
- Mutable state where immutable would suffice
- Side effects not properly isolated

### 2. **Code Duplication**
- Linux and BSD implementations share ~70% similar code
- Response building logic scattered across modules

### 3. **Insufficient Error Handling**
- Silent failures (e.g., lock poisoning)
- No timeout handling for slow clients
- Missing backpressure mechanisms

### 4. **Test Coverage Gaps**
- No tests for error conditions
- No tests for resource cleanup
- No performance regression tests
- Missing integration tests for real-world scenarios

## Refactoring Plan

### Phase 1: Critical Bug Fixes (Immediate)

1. **Fix BSD Event Processing**
   - Modify kqueue implementation to process multiple events
   - Add event batching for better performance

2. **Implement Dynamic Request Buffering**
   - Replace fixed buffer with growable buffer
   - Add configurable request size limits
   - Implement streaming parser for large requests

3. **Add Proper Resource Cleanup**
   - Implement RAII patterns for file descriptors
   - Add cleanup handlers for all error paths
   - Use Drop trait for automatic cleanup

### Phase 2: Functional Refactoring (Week 1)

1. **Replace Loops with Iterators**
   ```rust
   // Before
   for event in &events[..num_events] {
       // process
   }
   
   // After
   events[..num_events]
       .iter()
       .map(|event| process_event(event))
       .collect::<Result<Vec<_>, _>>()?;
   ```

2. **Immutable State Management**
   - Replace Mutex<HashMap> with Arc<DashMap> for connections
   - Use persistent data structures where appropriate
   - Implement state machines with enum variants

3. **Extract Common Logic**
   - Create trait for event loop implementations
   - Share connection handling logic between platforms
   - Implement functional error handling with Result chains

### Phase 3: Enhanced Testing (Week 2)

1. **Add Comprehensive Tests**
   - Property-based testing for protocol compliance
   - Stress tests for resource management
   - Chaos testing for error conditions

2. **Performance Benchmarks**
   - Add criterion benchmarks
   - Test different load patterns
   - Memory usage profiling

### Phase 4: Advanced Features (Week 3)

1. **Connection Pooling Improvements**
   - Implement fair scheduling
   - Add connection health checks
   - Implement graceful degradation

2. **Observability**
   - Add metrics collection
   - Implement distributed tracing
   - Add structured logging

## Immediate Actions

1. Run the bug demonstration tests: `cargo test --test bug_tests`
2. Apply critical fixes for BSD event processing and request size limitation
3. Add resource cleanup in error paths
4. Begin functional refactoring of core loops

## Long-term Improvements

1. Consider using tokio or async-std instead of custom event loops
2. Implement HTTP/2 support
3. Add WebSocket support
4. Create benchmarking suite for regression testing

## Conclusion

The nvo_servers library has a solid foundation but requires significant refactoring to address critical bugs and improve code quality. The proposed changes will make the codebase more maintainable, performant, and aligned with functional programming principles.