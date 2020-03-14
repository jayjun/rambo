# Changelog

### Added

- Add option to kill command after timeout

### Changed

- Switch to asynchronous I/O

### Fixes

- Subtle race conditions in logging
- Deadlocks after large input ([#1](https://github.com/jayjun/rambo/issues/1))

## 0.2.2

### Added

- Support iodata as standard input

## 0.2.1

### Fixes

- Resolve `priv` directory at runtime

## 0.2.0

### Added

- Stream command output with `:log` option
- Stop command with `kill/1`
- Add `:purge` setting to remove unused binaries

### Fixes

- Kill command if standard input to shim closes

## 0.1.0

- Initial release
