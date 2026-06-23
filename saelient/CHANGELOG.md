# Changelog

## [Unreleased]

- Implement slot f32 rounding manually for no_std platforms.

## v0.2.3

- Fix SAEpc03 and SAEpc04 slots scale and offset incorrect.

## v0.2.2

- Fix EDP bit being masked off when getting the PGN from an id.
- Assert TP.RTS packets per response is at least one.
- Also deserialize `AbortSenderRole::Reserved` rather than returning `Err`.
- Make PGN accessible for `RequestToSend`.
- SLOTs for SAEpc03 and SAEpc04.
- Rename defmt feature.

## v0.2.1

- Add SAEec06 and SAEec09 SLOTs.
- Add data page and extended data page to builder.
- Implement `PartialEq` to mask of priority bits when comparing ids.
- Fix spatial pointer flag not being set in memory request.
- Fix wrong binary operation in masking sender role bits for transport message.
- Fix SLOT forward transfer function order of operations.
- Module doc-comment titles.
- Improve unit test coverage across all modules.
- Update defmt to v1.1.
- Make tests only run with required features.

## v0.2.0

- Initial release.

## v0.1.0, v0.1.1

Old crate. Rewritten in v0.2.0.
