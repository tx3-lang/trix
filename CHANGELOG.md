# Changelog

All notable changes to this project will be documented in this file.

## [0.28.0] - 2026-09-28

### 🚀 Features

- *(codegen)* Register java client plugin (#133)
- *(codegen)* Register Swift client plugin (#134)

## [0.27.0] - 2026-09-26

### 🚀 Features

- *(codegen)* Render built-in plugins from tx3c's built-in templates (#132)

### 🐛 Bug Fixes

- *(publish)* Write OCI `created` into image config so registries order tags (#125)

### 💼 Other

- V0.27.0

### 🧪 Testing

- Restructure the local suite into unit/cli/contract layers (#129)

### ⚙️ Miscellaneous Tasks

- Harden Rust CI/CD (#126)
- Make CI almost green (#127)
- Enforce the fmt/clippy/test gates and lock the test run (#130)

## [0.26.2] - 2026-06-17

### 🐛 Bug Fixes

- *(invoke)* Exit non-zero when cshell resolve fails (#124)

### 💼 Other

- V0.26.2

## [0.26.1] - 2026-06-16

### 🐛 Bug Fixes

- *(test)* Repair `trix test` expect/balance phase + modernize utxorpc utxo parsing (#123)

### 💼 Other

- V0.26.1

## [0.26.0] - 2026-06-15

### 🚀 Features

- Enforce project-declared minimum tx3c version from trix.toml (#121)

### 💼 Other

- V0.26.0

### ⚙️ Miscellaneous Tasks

- Raise tx3c compat floor to 0.22.0 (#122)

## [0.25.1] - 2026-05-29

### 🐛 Bug Fixes

- *(publish)* Lowercase OCI repository path for uppercase scopes (#120)

### 💼 Other

- V0.25.1

## [0.25.0] - 2026-05-29

### 🚀 Features

- Simplify steps required to get the consumer flow running (#117)
- *(codegen)* Skip project target when main.tx3 is missing (#119)

### 🐛 Bug Fixes

- *(use)* Accept image/png logo layer when pulling protocol images (#118)

### 💼 Other

- V0.25.0

## [0.24.0] - 2026-05-27

### 🚀 Features

- Attach optional logo PNG layer at publish time (#115)

### 🐛 Bug Fixes

- Drop nested block_on in publish, await OCI push directly (#116)

### 💼 Other

- V0.24.0

## [0.23.1] - 2026-05-27

### 💼 Other

- V0.23.1

### ⚙️ Miscellaneous Tasks

- Remove unstable feature gate from publish command (#114)

## [0.23.0] - 2026-05-26

### 🚀 Features

- Anchor protocol identity to a GitHub repo at publish time (#112)
- Scaffold identity-trust mechanism for external interfaces (#113)

### 💼 Other

- V0.23.0

### ⚙️ Miscellaneous Tasks

- Update tx3up install URL after up→tx3up rename (#111)

## [0.22.0] - 2026-05-19

### 🚀 Features

- Make consuming external protocols 1st-class (#109)
- Delegate low-level ops to the tx3c binary (#110)

### 💼 Other

- V0.22.0

## [0.21.1] - 2026-03-21

### 🚀 Features

- Add TII support in publish command (#107)

### 🐛 Bug Fixes

- *(codegen)* Bring back legacy codegen while we update sdks (#108)

### 💼 Other

- V0.21.1

## [0.21.0] - 2026-03-18

### 💼 Other

- V0.21.0

### 🚜 Refactor

- *(codegen)* Delegate rendering to tx3c (#104)

### ⚙️ Miscellaneous Tasks

- Update dolos to v1 and adjust config template (#106)

## [0.20.0] - 2026-02-14

### 🚀 Features

- Introduce profile view command (#101)

### 🐛 Bug Fixes

- Pass built-in profiles to tx3c build command

### 💼 Other

- V0.20.0

### 🧪 Testing

- Create e2e harness and initial tests (#102)
- Tidy up cshell provider test

## [0.19.7] - 2026-02-10

### 🐛 Bug Fixes

- Update default Dolos config to match latest schema

### 💼 Other

- V0.19.7

### ⚙️ Miscellaneous Tasks

- Update dolos to v1.0.0-rc.9 (#100)

## [0.19.6] - 2026-01-08

### 🐛 Bug Fixes

- Use default function for timeout in TelemetryConfig

### 💼 Other

- V0.19.6

## [0.19.5] - 2026-01-07

### 🐛 Bug Fixes

- Increase default timeout for telemetry

### 💼 Other

- V0.19.5

## [0.19.4] - 2026-01-06

### 🚀 Features

- Introduce lightweight telemetry (#96)

### 🐛 Bug Fixes

- *(telemetry)* Send metrics using delta aggregation temporality

### 💼 Other

- V0.19.4

### ⚙️ Miscellaneous Tasks

- Tidy up design folder

## [0.19.3] - 2026-01-02

### 🚀 Features

- *(devnet)* Improve UX when no devnet config is available (#95)

### 🐛 Bug Fixes

- Use correct convention for u5c port

### 💼 Other

- V0.19.3

### 🚜 Refactor

- Use target dir for dolos config files

## [0.19.2] - 2025-12-31

### 💼 Other

- V0.19.2

### ⚙️ Miscellaneous Tasks

- Update tx3 deps to v0.14.2

## [0.19.1] - 2025-12-29

### 🐛 Bug Fixes

- Pass implicit profiles to tx3c
- Include placeholder for ledger family config

### 💼 Other

- V0.19.1

## [0.19.0] - 2025-12-29

### 🚀 Features

- Introduce devnet copy command (#84)
- Add u5c endpoint headers support
- Load profile-specific env vars when required (#87)
- Support custom tool path via env vars (#90)

### 🐛 Bug Fixes

- Use correct `signers` args in cshell invoke (#82)
- Improve compatibility with previous config formats

### 💼 Other

- V0.19.0

### 🚜 Refactor

- Apply alpha learnings to beta version (#94)

### ⚙️ Miscellaneous Tasks

- Update tx3 to v0.12 (#86)
- Update tx3 and u5c deps
- Remove x86 apple binary build

## [0.18.0] - 2025-08-06

### 🐛 Bug Fixes

- Use cshell for wallet data
- Clean-up noise on invoke stdout

### 💼 Other

- V0.18.0

## [0.17.0] - 2025-08-06

### 🚀 Features

- Report tx3up updates when available

### 💼 Other

- V0.17.0

## [0.16.0] - 2025-08-04

### 💼 Other

- V0.16.0

### 🚜 Refactor

- Split devnet config file (#79)
- Adapt to new cshell invoke command (#80)

### ⚙️ Miscellaneous Tasks

- Apply small QoL adjustments (#81)

## [0.15.0] - 2025-07-31

### 🚀 Features

- *(bindgen)* Support dynamic options, static files and multiple templates (#73)
- *(invoke)* Support passing args in json format (#77)

### 💼 Other

- V0.15.0

### ⚙️ Miscellaneous Tasks

- *(bindgen)* Fix template sources to specific commit hash (#74)
- *(bindgen)* Use tags to point to specific plugin commits (#78)
- Update tx3 to v0.11.0

## [0.14.0] - 2025-07-22

### 💼 Other

- V0.14.0

### ⚙️ Miscellaneous Tasks

- Update tx3 deps to v0.10.0

## [0.13.0] - 2025-07-18

### 🚀 Features

- Implement opt out mechanism for telemetry (#69)
- Enhance build command (#56)
- Add evergreen notifications (#68)
- *(bindgen)* Add support for plugin options in trix.toml (#70)

### 🐛 Bug Fixes

- Use correct default TRP endpoint (#72)
- Adjust to latest tx3 IR types

### 💼 Other

- V0.13.0

### ⚙️ Miscellaneous Tasks

- Update tx3-lang to v0.9.0
- Remove update checker now migrated to tx3up

## [0.12.0] - 2025-07-11

### 🚀 Features

- Introduce inspect cmd (#64)
- Introduce wallet command (#67)

### 💼 Other

- V0.12.0

### ⚙️ Miscellaneous Tasks

- Set default registry url (#61)

## [0.11.2] - 2025-07-07

### 💼 Other

- V0.11.2

## [0.11.1] - 2025-07-05

### 💼 Other

- V0.11.1

### ⚙️ Miscellaneous Tasks

- Update tx3-lang to v0.7.1

## [0.11.0] - 2025-07-04

### 🚀 Features

- Add publish command (#38)

### 🐛 Bug Fixes

- Apply upstream changes to AST identity names (#57)
- Make bindgen cmd async to avoid nested tokio runtime (#58)

### 💼 Other

- V0.11.0

### ⚙️ Miscellaneous Tasks

- Automate changelog via git-cliff
- Update deprecated windows runner label

## [0.10.0] - 2025-06-06

### 🐛 Bug Fixes

- Use new type defined in go bindgen template (#50)

### 💼 Other

- V0.10.0

### ⚙️ Miscellaneous Tasks

- Update tx3 deps to v0.6.0 (#52)

## [0.9.1] - 2025-06-02

### 🐛 Bug Fixes

- Use deterministic wallet keys on each devnet run (#43)

### 💼 Other

- V0.9.1

### ⚙️ Miscellaneous Tasks

- Implement tx3 action and test workflow (#49)

## [0.9.0] - 2025-05-30

### 🚀 Features

- Support non-interactive init command (#48)

### 💼 Other

- V0.9.0

### ⚙️ Miscellaneous Tasks

- Update cshell spawn to use new tx command (#42)

## [0.8.0] - 2025-05-23

### 🚀 Features

- Introduce build command (#32)

### 🐛 Bug Fixes

- Make test hashed folder deterministic (#33)
- Use correct path convention for wallet create (#34)
- Fix cshell config path when triggering a devnet (#36)
- Fill gaps in devnet templates (#37)

### 💼 Other

- V0.8.0

### ⚙️ Miscellaneous Tasks

- Update tx3-lang to v0.5.0 (#35)

## [0.7.0] - 2025-05-16

### 🚀 Features

- Introduce trix test command (#21)

### 🐛 Bug Fixes

- Fix test template syntax (#29)
- Use correct args for explore command (#30)
- Use correct args for invoke command (#31)

### 💼 Other

- V0.7.0

### 🚜 Refactor

- Abstract the interaction with child processes (#28)

### ⚙️ Miscellaneous Tasks

- Remove small duplicate (#23)

## [0.6.1] - 2025-05-13

### 🐛 Bug Fixes

- Support init with or without previous config (#22)

### 💼 Other

- V0.6.1

## [0.6.0] - 2025-05-12

### 🚀 Features

- Use git to retrieve bindgen templates
- *(bindgen)* Add TIR version to template data (#19)
- Add version command to CLI (#20)

### 💼 Other

- V0.6.0

### ⚙️ Miscellaneous Tasks

- Update tx3-lang to v0.4.0 (#18)

## [0.5.2] - 2025-05-02

### 💼 Other

- V0.5.2

### ⚙️ Miscellaneous Tasks

- Update tx3-lang to v0.3.0 (#16)

## [0.5.1] - 2025-04-23

### 💼 Other

- V0.5.1

### ⚙️ Miscellaneous Tasks

- Update trp public keys (#13)

## [0.5.0] - 2025-04-23

### 🚀 Features

- Implement invoke command (#12)

### 💼 Other

- V0.5.0

## [0.4.0] - 2025-04-23

### 🚀 Features

- *(bindgen)* Provide node package files for typescript (#8)
- Implemented devnet commands (#10)
- Define reasonable profile defaults (#11)

### 🐛 Bug Fixes

- Update main and types fields to use project_name template (#9)

### 💼 Other

- V0.4.0

## [0.3.0] - 2025-04-22

### 🚀 Features

- *(init)* Create init command logic (#7)

### 💼 Other

- V0.3.0

## [0.2.0] - 2025-04-20

### 🚀 Features

- Implement basic check command (#3)
- Implement basic bindgen command (#4)

### 💼 Other

- V0.2.0

### ⚙️ Miscellaneous Tasks

- Scaffold rust project
- Scaffold cli commands (#2)
- Setup binary release (#1)
- Update dist config (#5)
- Setup cargo release
- Add onchain example WIP (#6)
- Update dist workers
- Remove unnecessary steps

<!-- generated by git-cliff -->
