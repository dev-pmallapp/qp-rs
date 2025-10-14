# QP Framework for Embedded Systems in Rust
This repository contains the Rust port of the QP real-time embedded framework, originally developed by Quantum Leaps in C/C++.

## QP Framework Overview
The QP framework is a family of lightweight, open-source, real-time embedded frameworks (RTEFs) implementing the Active Object (Actor) design pattern with UML hierarchical state machines. The framework provides:

- **Active Objects**: Encapsulated, event-driven concurrent objects
- **Hierarchical State Machines**: UML statechart implementation for complex behavior modeling
- **Real-Time Kernels**: Multiple scheduling options (cooperative, preemptive, dual-mode)
- **Event-Driven Architecture**: Asynchronous message passing between active objects
- **Memory Management**: Static memory allocation suitable for embedded systems
- **Tracing & Testing**: Built-in software tracing and unit testing capabilities

## Rust Port Objectives
Create a safe, zero-cost abstraction Rust implementation that maintains the real-time deterministic behavior of the original C/C++ framework while leveraging Rust's memory safety and concurrency features.

## Architecture Overview

### Core Components to Port
1. **QEP (Event Processor)**: State machine engine and event handling
2. **QF (Framework)**: Active object container and event management  
3. **QV/QK/QXK Kernels**: Cooperative, preemptive, and dual-mode schedulers
4. **QS (Spy)**: Software tracing and debugging infrastructure
5. **Memory Pools**: Static memory management for events and objects
6. **Time Management**: Timeouts, time events, and tick processing

### Rust-Specific Design Principles
- **Zero-cost abstractions**: Compile-time optimizations with no runtime overhead
- **Memory safety**: Leverage Rust's ownership system to prevent common embedded bugs
- **no_std compatibility**: Support for bare-metal embedded targets
- **Type safety**: Use Rust's type system to prevent state machine design errors
- **Trait-based design**: Flexible, composable interfaces for different kernel types

## Detailed Task Breakdown

### Phase 1: Foundation Layer
#### Task 1.1: Project Structure Setup
- [ ] Create Cargo workspace with separate crates for each QP component
- [ ] Set up `no_std` + `no_main` configuration for embedded targets
- [ ] Configure cross-compilation targets (ARM Cortex-M, RISC-V)
- [ ] Establish CI/CD pipeline with embedded testing on QEMU
- [ ] Create feature flags for different kernel types and capabilities

#### Task 1.2: Core Type Definitions
- [ ] Define `QEvent` trait and event hierarchy using Rust enums
- [ ] Implement `QSignal` type-safe event signal definitions
- [ ] Create `QState` function pointer type for state handlers
- [ ] Design `QActive` trait for active objects with associated types
- [ ] Define `QPriority` and `QTimeEvtCtr` newtypes for type safety

#### Task 1.3: Memory Management Foundation  
- [ ] Port QF memory pools using `heapless` data structures
- [ ] Implement static event allocation with compile-time sizing
- [ ] Create `QEvt` smart pointer type with automatic cleanup
- [ ] Design memory pool traits for different allocation strategies
- [ ] Add memory usage tracking and debugging support

### Phase 2: Event Processing Engine (QEP)
#### Task 2.1: State Machine Core
- [ ] Port hierarchical state machine engine to Rust
- [ ] Implement state transition mechanics with compile-time verification
- [ ] Create state handler dispatch using function pointers and closures  
- [ ] Design state entry/exit action handling
- [ ] Add support for state machine introspection and debugging

#### Task 2.2: Event Handling
- [ ] Implement event dispatch with zero-cost abstractions
- [ ] Create deferred event handling mechanism
- [ ] Add event recycling and reference counting
- [ ] Design publish-subscribe event delivery
- [ ] Implement event queue management with bounded collections

#### Task 2.3: State Machine Macros
- [ ] Create declarative macros for state machine definition
- [ ] Implement procedural macros for automatic state binding
- [ ] Add compile-time state machine validation
- [ ] Generate transition tables and optimized dispatch code
- [ ] Provide debugging and visualization macro attributes

### Phase 3: Framework Layer (QF)
#### Task 3.1: Active Object Implementation
- [ ] Port `QActive` base class functionality to Rust traits
- [ ] Implement active object lifecycle management
- [ ] Create event queue management with different queue types
- [ ] Add priority-based event processing
- [ ] Design active object registry and lookup mechanisms

#### Task 3.2: Event Management System
- [ ] Implement event pools with different sizing strategies
- [ ] Create garbage collection for dynamic events
- [ ] Add event reference counting and automatic cleanup
- [ ] Design event multicasting and broadcasting
- [ ] Implement event filtering and routing capabilities

#### Task 3.3: Time Event System
- [ ] Port time event management to Rust
- [ ] Implement periodic and one-shot timers
- [ ] Create tick processing with configurable tick rates
- [ ] Add time event scheduling with priority queues
- [ ] Design timeout handling and expiration callbacks

### Phase 4: Real-Time Kernels
#### Task 4.1: QV Cooperative Kernel
- [ ] Port vanilla cooperative scheduler
- [ ] Implement run-to-completion semantics  
- [ ] Create event loop with priority-based dispatching
- [ ] Add idle processing and power management hooks
- [ ] Design interrupt handling integration

#### Task 4.2: QK Preemptive Kernel
- [ ] Port preemptive priority-based scheduler
- [ ] Implement ceiling priority protocol for mutex-free operation
- [ ] Create interrupt-driven preemption handling
- [ ] Add priority inheritance and inversion avoidance
- [ ] Design stack management and overflow detection

#### Task 4.3: QXK Dual-Mode Kernel  
- [ ] Port extended kernel with basic thread support
- [ ] Implement hybrid active object/thread model
- [ ] Create blocking services for extended threads
- [ ] Add semaphore and mutex primitives
- [ ] Design thread priority management

### Phase 5: Development & Debugging Support
#### Task 5.1: QS Software Tracing
- [ ] Port QS tracing infrastructure to Rust
- [ ] Implement zero-overhead tracing when disabled
- [ ] Create trace event formatting and filtering
- [ ] Add real-time trace streaming over various interfaces
- [ ] Design trace analysis and visualization tools

#### Task 5.2: Testing Framework
- [ ] Create QUTest equivalent for Rust unit testing
- [ ] Implement test fixtures for active objects and state machines
- [ ] Add mock objects and event injection capabilities
- [ ] Create automated test execution on embedded targets
- [ ] Design property-based testing for state machine verification

#### Task 5.3: Integration Tools
- [ ] Create BSP (Board Support Package) abstraction layer
- [ ] Implement HAL integration for common microcontrollers
- [ ] Add RTOS integration layer (FreeRTOS, Embassy, RTIC)
- [ ] Create device driver framework compatible with QP patterns
- [ ] Design configuration and code generation tools

### Phase 6: Platform Ports & Examples
#### Task 6.1: Embedded Platform Support
- [ ] Port to ARM Cortex-M using `cortex-m` crate
- [ ] Add RISC-V support with appropriate HAL integration
- [ ] Create STM32 family ports with peripheral integration
- [ ] Implement nRF52/nRF53 ports for wireless applications
- [ ] Add ESP32 support for IoT applications

#### Task 6.2: Example Applications
- [ ] Port "Blinky" example as basic functionality demonstration
- [ ] Create "Dining Philosopher Problem" (DPP) showcase  
- [ ] Implement "Calculator" example with GUI state machine
- [ ] Add IoT sensor network example with wireless communication
- [ ] Create safety-critical example following automotive standards

#### Task 6.3: Performance Validation
- [ ] Benchmark against original C/C++ implementation
- [ ] Measure memory footprint and execution overhead
- [ ] Validate real-time behavior and determinism
- [ ] Test interrupt latency and response times
- [ ] Create performance regression testing suite

### Phase 7: Documentation & Ecosystem
#### Task 7.1: Documentation
- [ ] Write comprehensive API documentation with examples
- [ ] Create migration guide from QP/C and QP/C++
- [ ] Add tutorials for embedded Rust developers new to QP
- [ ] Design state machine modeling guide for Rust
- [ ] Create troubleshooting and debugging documentation

#### Task 7.2: Tooling Integration
- [ ] Integrate with existing Rust embedded ecosystem (`probe-rs`, `defmt`)
- [ ] Create `cargo` templates for QP projects
- [ ] Add IDE support and debugging configurations
- [ ] Design QM modeling tool Rust code generation
- [ ] Implement static analysis and linting rules

## Success Criteria
- [ ] Feature parity with QP/C 8.x in terms of functionality
- [ ] Zero-cost abstraction with performance within 5% of C implementation
- [ ] Memory safety with no runtime panics in well-formed programs
- [ ] Support for major embedded Rust platforms (Cortex-M, RISC-V)
- [ ] Comprehensive test suite with >95% code coverage
- [ ] Production-ready examples demonstrating real-world usage patterns
