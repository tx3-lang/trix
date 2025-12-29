# Changelog

All notable changes to this project will be documented in this file.

## [0.19.0] - 2025-12-29

### 🚀 Features

- Introduce devnet copy command (#84)
- Add u5c endpoint headers support
- Load profile-specific env vars when required (#87)
- Support custom tool path via env vars (#90)

### 🐛 Bug Fixes

- Use correct `signers` args in cshell invoke (#82)
- Improve compatibility with previous config formats

### 🚜 Refactor

- Apply alpha learnings to beta version (#94)

### ⚙️ Miscellaneous Tasks

- Update tx3 to v0.12 (#86)
- Update tx3 and u5c deps

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
