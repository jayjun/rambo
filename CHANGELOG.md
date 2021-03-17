# 0.3.4

## Bug Fixes

- Fix typespecs

# 0.3.3

## Bug Fixes

- Fix typespec

# 0.3.2

## Bug Fixes

- Fail gracefully if child standard I/O cannot open
- Collect output using iolists

# 0.3.1

## Bug Fixes

- Fix wrong IO device name

# 0.3.0

## Enhancements

- Add option to kill command after timeout
- Switch to asynchronous I/O

## Bug Fixes

- Subtle race conditions in logging
- Deadlocks after large input ([#1](https://github.com/jayjun/rambo/issues/1))

# 0.2.2

## Enhancements

- Support iodata as standard input

# 0.2.1

## Enhancements

- Resolve `priv` directory at runtime

# 0.2.0

## Enhancements

- Stream command output with `:log` option
- Stop command with `kill/1`
- Add `:purge` setting to remove unused binaries

## Bug Fixes

- Kill command if standard input to shim closes

# 0.1.0

- Initial release
