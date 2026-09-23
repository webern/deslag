
<a name="4.1.3"></a>
## [4.1.3](https://gitlab.com/AdrianDC/gitlabci-local/compare/4.1.2...4.1.3) (2021-07-12)

### Bug Fixes

* finish [#187](https://gitlab.com/AdrianDC/gitlabci-local/issues/187): properly detect the UTF-8 stdout encoding

### Cleanups

* coverage: remove unused function 'docker / _container'

### Documentation

* resolve [#189](https://gitlab.com/AdrianDC/gitlabci-local/issues/189): explicit support for Git Bash / CMD on Windows


<a name="4.1.2"></a>
## [4.1.2](https://gitlab.com/AdrianDC/gitlabci-local/compare/4.1.1...4.1.2) (2021-07-11)

### Bug Fixes

* resolve [#187](https://gitlab.com/AdrianDC/gitlabci-local/issues/187): check support for non-UTF-8 histories outputs
* resolve [#187](https://gitlab.com/AdrianDC/gitlabci-local/issues/187): check support for non-UTF-8 boxes outputs

### Cleanups

* gitlab-ci: restore needs: 'Coverage Windows' for SonarCloud


<a name="4.1.1"></a>
## [4.1.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/4.1.0...4.1.1) (2021-07-11)

### Bug Fixes

* resolve [#192](https://gitlab.com/AdrianDC/gitlabci-local/issues/192): also merge the 'default' node as additions
* resolve [#192](https://gitlab.com/AdrianDC/gitlabci-local/issues/192): support empty included files merges

### Cleanups

* gitlab-ci: fix 'Coverage Windows' issues with pip and wheel
* requirements: use my fixed 'gitlab-release' personal fork
* tests: run the 'disabled' tests on the native host
* lint: reduce and resolve some pylint disabled rules
* lint: resolve all new pylint warnings in the sources
* prepare [#184](https://gitlab.com/AdrianDC/gitlabci-local/issues/184): refactor '__globals' without iterators

### Features

* implement [#184](https://gitlab.com/AdrianDC/gitlabci-local/issues/184): add support for the 'default:' node

### Test

* finish [#191](https://gitlab.com/AdrianDC/gitlabci-local/issues/191): run 'includes/variables' test and run on host


<a name="4.1.0"></a>
## [4.1.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/4.0.0...4.1.0) (2021-07-08)

### Bug Fixes

* prepare [#181](https://gitlab.com/AdrianDC/gitlabci-local/issues/181): drop support for engine partial name inputs
* resolve [#185](https://gitlab.com/AdrianDC/gitlabci-local/issues/185): use the 'podman' binary variable in 'cmd_exec'
* resolve [#183](https://gitlab.com/AdrianDC/gitlabci-local/issues/183): ensure multiple global services are all added

### Cleanups

* gitlab-ci: disable 'Coverage Windows' for the moment
* vscode: ensure Prettier formatters use single quotes only
* tests: reduce some tests duration with native runs
* histories: use fake durations for 'time' coverage tests
* tests: configure 'pexpect-executor' delays to reduce times
* gitlab-ci: add tests execution times with a 'time' wrapper
* gitlab-ci: improve 'Python Local' tests with sudo installs
* finish [#185](https://gitlab.com/AdrianDC/gitlabci-local/issues/185): avoid the unreliable __del__ 'remove()' calls
* gitlab-ci: ensure 'Build' runs without jobs dependencies
* gitlab-ci: run tests only on Python 3.6 (old) and 3.9 (new)
* types: disable coverage of Windows specific or unused codes
* cleanup [#181](https://gitlab.com/AdrianDC/gitlabci-local/issues/181): drop the unused 'engine.services' property
* prepare [#185](https://gitlab.com/AdrianDC/gitlabci-local/issues/185): handle the 'sockets' feature at engine level
* prepare [#181](https://gitlab.com/AdrianDC/gitlabci-local/issues/181): pass 'services' and script folder to 'run()'
* finish [#185](https://gitlab.com/AdrianDC/gitlabci-local/issues/185): refactor with container members and properties
* gitlab-ci: migrate to Podman 3.0.x configuration files
* gitlab-ci: use 'needs' instead of 'dependencies' for tests
* gitlab-ci: ignore Pylint 'duplicate-code' warnings
* prepare [#181](https://gitlab.com/AdrianDC/gitlabci-local/issues/181): add 'quote' method for the Strings class
* prepare [#181](https://gitlab.com/AdrianDC/gitlabci-local/issues/181): get 'random' strings with letters and digits
* prepare [#185](https://gitlab.com/AdrianDC/gitlabci-local/issues/185): isolate Volumes string builder to 'stringify'
* prepare [#185](https://gitlab.com/AdrianDC/gitlabci-local/issues/185): use 'name' and 'folder' as Scripts properties
* prepare [#185](https://gitlab.com/AdrianDC/gitlabci-local/issues/185): import engine modules per class directly
* prepare [#183](https://gitlab.com/AdrianDC/gitlabci-local/issues/183): extract the services 'alias' key as well
* prepare [#183](https://gitlab.com/AdrianDC/gitlabci-local/issues/183): hold services name in a data dictionnary
* platform: minor codestyle improvement for print flush
* markdownlint: extend line lengths to 150 characters max
* gitlab-ci: use the standard 'docker:dind' service image
* gitlab-ci: minor codestyle cleanups of requirements

### Features

* resolve [#191](https://gitlab.com/AdrianDC/gitlabci-local/issues/191): add support for 'variables:' includes
* prepare [#181](https://gitlab.com/AdrianDC/gitlabci-local/issues/181): bind DinD sockets if services are unsupported
* implement [#182](https://gitlab.com/AdrianDC/gitlabci-local/issues/182): add support for migration paths in 'Updates'
* implement [#183](https://gitlab.com/AdrianDC/gitlabci-local/issues/183): pull the containers images of 'services:'

### Test

* finish [#183](https://gitlab.com/AdrianDC/gitlabci-local/issues/183): pull existing image in sockets services tests


<a name="4.0.0"></a>
## [4.0.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/3.1.2...4.0.0) (2021-02-21)

### Bug Fixes

* finish [#171](https://gitlab.com/AdrianDC/gitlabci-local/issues/171): fixup MSYS paths translations upon volumes parse
* finish [#171](https://gitlab.com/AdrianDC/gitlabci-local/issues/171): support ';' separated git-bash paths expansions
* resolve [#171](https://gitlab.com/AdrianDC/gitlabci-local/issues/171): resolve support of Windows paths volumes mounts
* finish [#179](https://gitlab.com/AdrianDC/gitlabci-local/issues/179): avoid resolving ~ on Windows hosts and refactor
* resolve [#179](https://gitlab.com/AdrianDC/gitlabci-local/issues/179): expand home paths for volumes and workdir
* resolve [#178](https://gitlab.com/AdrianDC/gitlabci-local/issues/178): support all types of include: configurations
* finish [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): remove unneeded 'dict' iterator on services type
* finish [#177](https://gitlab.com/AdrianDC/gitlabci-local/issues/177): add coverage after daily checks with a pipeline
* finish [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): add coverage of all environment variables
* finish [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): fix handlings of 'dict' or 'str' services lists
* finish [#177](https://gitlab.com/AdrianDC/gitlabci-local/issues/177): add coverage for daily checks after a pipeline
* resolve [#177](https://gitlab.com/AdrianDC/gitlabci-local/issues/177): prepare version values only if needed
* finish [#174](https://gitlab.com/AdrianDC/gitlabci-local/issues/174): add 'git' environment path and run local tests
* resolve [#175](https://gitlab.com/AdrianDC/gitlabci-local/issues/175): ensure when: always jobs always run
* finish [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): add support for the name: variant of 'services:'
* resolve [#173](https://gitlab.com/AdrianDC/gitlabci-local/issues/173): expand variables from the workdir value
* resolve [#172](https://gitlab.com/AdrianDC/gitlabci-local/issues/172): fix one part .local:volumes items
* prepare [#165](https://gitlab.com/AdrianDC/gitlabci-local/issues/165): consider nodes containing 'script' as jobs
* resolve [#164](https://gitlab.com/AdrianDC/gitlabci-local/issues/164): avoid --debug features if interrupted by Ctrl+C
* resolve [#163](https://gitlab.com/AdrianDC/gitlabci-local/issues/163): handle APIError fails in docker.supports()

### Cleanups

* coverage: disable coverage of Windows specific sections
* finish [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): disable coverage of function 'Outputs.warning'
* finish [#169](https://gitlab.com/AdrianDC/gitlabci-local/issues/169): remove the unused 'StageHistory.get' function
* gitlab-ci: add 'Python DinD' local tests job with DinD
* finish [#169](https://gitlab.com/AdrianDC/gitlabci-local/issues/169): use integer durations to avoid '1 seconds'
* finish [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): cleanup 'null' empty configurations fields
* features: isolate the pipelines filter into a function
* finish [#166](https://gitlab.com/AdrianDC/gitlabci-local/issues/166): disable coverage of DOCKER_HOST offline cases
* finish [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): unify the configurations cleanup in a function
* gitlab-ci: always push to SonarCloud on develop / master
* finish [#162](https://gitlab.com/AdrianDC/gitlabci-local/issues/162): mention real paths are available on macOS
* resolve [#162](https://gitlab.com/AdrianDC/gitlabci-local/issues/162): specific warnings about unsupported features
* document [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): mention sockets are enabled by dind services

### Documentation

* preview: refresh the SVG for the latest 4.0.0 release
* finish [#178](https://gitlab.com/AdrianDC/gitlabci-local/issues/178): document the 'include:' supported nodes
* finish [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): document the 'services:' supported nodes
* finish [#174](https://gitlab.com/AdrianDC/gitlabci-local/issues/174): document CI_COMMIT_SHA and CI_COMMIT_SHORT_SHA

### Features

* prepare [#118](https://gitlab.com/AdrianDC/gitlabci-local/issues/118): add an 'EXPERIMENTAL' to enable Docker sockets
* resolve [#176](https://gitlab.com/AdrianDC/gitlabci-local/issues/176): share DOCKER_CERT_PATH and DOCKER_TLS_VERIFY
* implement [#174](https://gitlab.com/AdrianDC/gitlabci-local/issues/174): set CI_COMMIT_SHA and CI_COMMIT_SHORT_SHA
* implement [#169](https://gitlab.com/AdrianDC/gitlabci-local/issues/169): add pipeline / jobs histories and refactor
* resolve [#168](https://gitlab.com/AdrianDC/gitlabci-local/issues/168): explicitly use docker.io registry for Podman
* implement [#167](https://gitlab.com/AdrianDC/gitlabci-local/issues/167): support Podman default network interface
* resolve [#165](https://gitlab.com/AdrianDC/gitlabci-local/issues/165): handle empty job stages and missing stages
* resolve [#161](https://gitlab.com/AdrianDC/gitlabci-local/issues/161): add support for Docker sockets on Windows
* resolve [#166](https://gitlab.com/AdrianDC/gitlabci-local/issues/166): add support for DOCKER_HOST sockets
* implement [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): enable the sockets option with dind services

### Revert

* finish [#168](https://gitlab.com/AdrianDC/gitlabci-local/issues/168): let fixed Podman resolve short-names again

### Test

* finish [#179](https://gitlab.com/AdrianDC/gitlabci-local/issues/179): minor fixes of 'home' tests with /root workdir
* finish [#179](https://gitlab.com/AdrianDC/gitlabci-local/issues/179): add a test with the "${PWD}" absolute path env
* finish [#165](https://gitlab.com/AdrianDC/gitlabci-local/issues/165): add 'trigger:' coverage with a faulty 'script:'

### Test

* test [#178](https://gitlab.com/AdrianDC/gitlabci-local/issues/178): validate all types of include: configurations
* test [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): resolve the DOCKER_HOST hostname to IP for DinD
* test [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): resolve DinD pull executions without a timeout
* test [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): adapt DOCKER_HOST for GitLab CI tests
* test [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): ensure DOCKER_HOST points to a working deamon
* test [#167](https://gitlab.com/AdrianDC/gitlabci-local/issues/167): add a coverage test for engine network modes
* test [#160](https://gitlab.com/AdrianDC/gitlabci-local/issues/160): run sockets tests only Docker supported hosts


<a name="3.1.2"></a>
## [3.1.2](https://gitlab.com/AdrianDC/gitlabci-local/compare/3.1.1...3.1.2) (2021-02-09)

### Bug Fixes

* resolve [#159](https://gitlab.com/AdrianDC/gitlabci-local/issues/159): support nested anchor scripts syntaxes


<a name="3.1.1"></a>
## [3.1.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/3.1.0...3.1.1) (2021-02-01)

### Bug Fixes

* resolve [#158](https://gitlab.com/AdrianDC/gitlabci-local/issues/158): prevent regex matches in interactive menus


<a name="3.1.0"></a>
## [3.1.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/3.0.2...3.1.0) (2021-01-30)

### Bug Fixes

* resolve [#156](https://gitlab.com/AdrianDC/gitlabci-local/issues/156): expand nested variables values like GitLab CI

### Cleanups

* docs: refresh the preview SVG for the latest 3.1.0 release
* gitlab-ci: synchronize stderr outputs with stdout outputs
* readme, test: add Android 11 to the tested environments
* readme: resolve a minor typo about --settings in README
* gitlab-ci: allow to use the 'SUITE' for regular tests jobs
* gitlab-ci: remove unnecessary 'wget' for 'Coverage Windows'
* test: minor codestyle improvements in TEST.md
* run: handle scripts failures upon job lines executions

### Features

* implement [#157](https://gitlab.com/AdrianDC/gitlabci-local/issues/157): see the job name upon result for readability

### Test

* gitlab-ci: fix Podman VFS storage driver with STORAGE_DRIVER
* test [#156](https://gitlab.com/AdrianDC/gitlabci-local/issues/156): resolve 'project' variables being now handled
* gitlab-ci: raise libseccomp2 to 2.5.1-1 for Podman tests


<a name="3.0.2"></a>
## [3.0.2](https://gitlab.com/AdrianDC/gitlabci-local/compare/3.0.1...3.0.2) (2020-12-23)

### Bug Fixes

* resolve [#121](https://gitlab.com/AdrianDC/gitlabci-local/issues/121): handle broken pipe upon logs outputs

### Cleanups

* parsers: refactor 'parse()' into separated methods
* jobs: refactor 'run()' into an 'Outputs' class and methods
* vscode: ignore '.ropeproject' folder from tracked files
* bundle,jobs: isolate env binary paths and jobs variables
* jobs: isolate script sources to a 'Scripts' class
* jobs: isolate 'runner' function to a 'Jobs' class
* features: rename 'jobs' feature to 'ConfigurationsFeature'
* features: isolate 'select' and 'configure' to 'Menus' class
* features: fix 'PipelinesFeature' feature class name
* parsers: isolate 'parse' and 'stage' to the 'GitLab' class
* parsers: isolate 'parser.read' to a 'Parsers' class
* features: turn the 'launcher' into a pipeline feature class
* gitlab-ci: run develop pipeline upon 'CHANGELOG.md' changes

### Test

* regex,simple: rename the jobs' stages to match the tests


<a name="3.0.1"></a>
## [3.0.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/3.0.0...3.0.1) (2020-12-22)

### Cleanups

* types: reduce unrequired nested if conditions
* cli: isolate the CLI main entrypoint to a cli/ submodule
* prints: isolate PyInquirer themes into a 'Menus' class

### Features

* implement [#155](https://gitlab.com/AdrianDC/gitlabci-local/issues/155): add arguments categories for readability


<a name="3.0.0"></a>
## [3.0.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.3.0...3.0.0) (2020-12-22)

### Bug Fixes

* resolve [#154](https://gitlab.com/AdrianDC/gitlabci-local/issues/154): preserve variables priority and override order
* resolve [#153](https://gitlab.com/AdrianDC/gitlabci-local/issues/153): ensure signals are restored and reraised
* resolve [#152](https://gitlab.com/AdrianDC/gitlabci-local/issues/152): avoid sudoer access from root user
* resolve [#151](https://gitlab.com/AdrianDC/gitlabci-local/issues/151): configurable WinPTY and limited coverage checks
* resolve [#151](https://gitlab.com/AdrianDC/gitlabci-local/issues/151): enforce WinPTY and improve coverage

### Cleanups

* docs: refactor the 'Preview' job into a 'termtosvg' job
* gitlab-ci: add a job-specific report to 'Coverage' jobs
* docs: use pexpect-executor 1.2.0 to hold the final prompt
* test: finish parser coverage of .env environment override
* gitlab-ci: support ',' separated SUITE values for coverage
* test: finish 'variables' coverage of environment overrides
* test: finish 'extends' coverage with two 'variables:' nodes
* test: add empty '{before,after}_script' and 'script' tests
* test: add '--sockets' and host failures coverage tests
* finish [#151](https://gitlab.com/AdrianDC/gitlabci-local/issues/151): support non-WinPTY execution environments
* run: adapt 'run.sh' to missing sudo and wine support
* gitlab-ci: use 'pip3' instead of 'pip' in tests template
* test [#153](https://gitlab.com/AdrianDC/gitlabci-local/issues/153): test reraised signals and 'Files.clean' coverage
* gitlab-ci: unify template scripts and add stages comments
* engines: ignore 'exec()' from coverage rather than comment
* test [#152](https://gitlab.com/AdrianDC/gitlabci-local/issues/152): implement permissions tests for temp files
* gitlab-ci: unify local VSCode coverage to a common XML file
* types: ignore 'Volumes' Windows case from coverage results
* gitlab-ci: ensure coverage XML files use relative sources
* test: add sudoer '--debug' Podman engine test
* resolve [#150](https://gitlab.com/AdrianDC/gitlabci-local/issues/150): restrict Dicts iterators and improve coverage
* gitlab-ci: add 'Coverage Windows' tests with PyWine image
* coverage: add '.coveragerc' to strip Linux / Windows paths
* engines: refactor 'help' into 'cmd_exec' for coverage tests
* vscode: ignore '.tmp.entrypoint.*' files in VSCode
* coverage: ignore safety unused code lines
* test [#149](https://gitlab.com/AdrianDC/gitlabci-local/issues/149): add macOS simulated test for settings coverage
* implement [#149](https://gitlab.com/AdrianDC/gitlabci-local/issues/149): handle simulated settings for virtual tests
* prepare [#149](https://gitlab.com/AdrianDC/gitlabci-local/issues/149): add simulated macOS environment and cleanup
* gitlab-ci: hide pip warnings and coverage report errors
* gitlab-ci: use updated 'docker:19-dind' image for 19.03.14
* gitlab-ci: set host and tool envs for pexpect-executor
* vscode: disable chords terminal features to allow Ctrl+K
* changelog: configure groups titles detailed map for chglog
* changelog: add a cleanup option to hide changelog commits

### Documentation

* readme: add missing modules dependencies and references
* readme: minor codestyle cleanups of the Linux support table

### Test

* images: use pexpect-executor to pull with an interactive TTY


<a name="2.3.0"></a>
## [2.3.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.2.3...2.3.0) (2020-12-14)

### Bug Fixes

* resolve [#148](https://gitlab.com/AdrianDC/gitlabci-local/issues/148): handle JSON or YAML string as unique choice
* resolve [#146](https://gitlab.com/AdrianDC/gitlabci-local/issues/146): ensure before_script really checks for issues
* resolve [#147](https://gitlab.com/AdrianDC/gitlabci-local/issues/147): default YAML or JSON value if non-interactive
* resolve [#145](https://gitlab.com/AdrianDC/gitlabci-local/issues/145): Handle configurations Dicts index out of range
* finish [#137](https://gitlab.com/AdrianDC/gitlabci-local/issues/137): delete temporary files only if they still exist

### Cleanups

* gitlab-ci: raise interactive tests timeout to 15 minutes
* coverage: ignore unused PyInquirer patcher lines coverage
* tests: add interactive unit tests with pexpect-executor
* docs: resolve configurations test's 12th value support
* lint: isolate and identify 'Modules libraries' imports
* features: prevent YAML dump outputs lines from wrapping
* tests: migrate to pexpect-executor 1.0.1 with tests support
* features: isolate 'dumper' into a 'Jobs' feature
* runner: remove unused engine logs reader and try except
* tests: use 'ubuntu:20.04' for --bash/--debug for bash tests
* tests: add time tests for 60+ seconds pipelines coverage
* tests: add multiple unit tests to improve sources coverage
* parser: handle 'FileNotFoundError' upon file parser
* tests: add unknown configurations test and raise error
* gitlab-ci: unify coverage reports, unify and common scripts
* version: exclude version '0.0.0' fallback from coverage
* gitlab-ci: run coverage and SonarCloud upon tests/ changes
* gitlab-ci: silent and hide all installation irrelevant logs
* tests: add '--settings' specific tests and install 'sudo'
* tests: add missing or incompatible Podman engine tests
* tests: add 'gitlabci-local -i' with regex name tests
* gitlab-ci: remove 'mount' command execution in all tests
* tests: add 'gitlabci-local -c ./folder/' arguments test
* engine: disable the engine.exec command until required
* gitlab-ci: isolate coverage databses and allow suite tests
* gitlab-ci: resolve 'SonarCloud' changes rules on develop
* vscode: exclude intermediate files from the project view
* vscode: migrate to 'brainfit.vscode-coverage-highlighter'
* coverage: ignore coverage of unreachable input securities
* gitlab-ci: implement Python coverage reports for SonarCloud
* gitlab-ci: add 'Py3.9 Preview' test of ./docs/preview.py
* docs: migrate to the isolated 'pexpect-executor' package
* parser: cleanup duplicated environment file checks
* tests: refactor and isolate all unit tests
* version: support non-packaged sources version fallback
* finish [#142](https://gitlab.com/AdrianDC/gitlabci-local/issues/142): isolate pull and rmi into a feature class
* engine: add support for -E '' as being default engines
* tests: add 'engines' tests from arguments and environment
* gitlab-ci: add --settings and wrapped --update-check tests
* gitlab-ci: create YAML anchors to reuse templates scripts
* readme: format the markdown sources automatically
* gitlab-ci: wrap preview.py delay out of the preview script
* requirements: isolate all requirements to a folder
* resolve [#141](https://gitlab.com/AdrianDC/gitlabci-local/issues/141): refactor and fix SonarQube issues for except:
* resolve [#141](https://gitlab.com/AdrianDC/gitlabci-local/issues/141): refactor and fix SonarQube issues in parser

### Features

* updates: improve updates colors and embed new test flags
* finish [#144](https://gitlab.com/AdrianDC/gitlabci-local/issues/144): add missing regex check for -i case option
* implement [#144](https://gitlab.com/AdrianDC/gitlabci-local/issues/144): add -i to ignore jobs name case distinctions
* implement [#143](https://gitlab.com/AdrianDC/gitlabci-local/issues/143): add --force to force pull container images
* implement [#142](https://gitlab.com/AdrianDC/gitlabci-local/issues/142): add --rmi to remove container images


<a name="2.2.3"></a>
## [2.2.3](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.2.2...2.2.3) (2020-12-10)

### Cleanups

* gitlab-ci: resolve Podman 2.2.1 issues in Debian 10.6
* gitlab-ci: prevent Podman unit tests to use Docker host
* readme: add pipeline and SonarCloud badges
* resolve [#141](https://gitlab.com/AdrianDC/gitlabci-local/issues/141): minor codestyle cleanups raised by SonarCloud
* resolve [#141](https://gitlab.com/AdrianDC/gitlabci-local/issues/141): resolve SonarQube issue in engines.wait
* gitlab-ci.yml: add support for SonarCloud analysis
* gitlab-ci: run build and tests jobs only if needed


<a name="2.2.2"></a>
## [2.2.2](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.2.1...2.2.2) (2020-12-09)

### Bug Fixes

* resolve [#137](https://gitlab.com/AdrianDC/gitlabci-local/issues/137): ensure temporary scripts are always deleted
* resolve [#139](https://gitlab.com/AdrianDC/gitlabci-local/issues/139): support readonly parent folders for entrypoints
* resolve [#138](https://gitlab.com/AdrianDC/gitlabci-local/issues/138): reset colors once the boxes are printed

### Cleanups

* gitlab-ci: ignore Podman issues until podman-2.2.1 is fixed
* resolve [#140](https://gitlab.com/AdrianDC/gitlabci-local/issues/140): add 'Platform.IS_ANDROID' unused constant

### Documentation

* resolve [#140](https://gitlab.com/AdrianDC/gitlabci-local/issues/140): add Android test environment explanations
* resolve [#140](https://gitlab.com/AdrianDC/gitlabci-local/issues/140): mention Android native engine with Termux
* prepare [#140](https://gitlab.com/AdrianDC/gitlabci-local/issues/140): add installation steps for all test platforms


<a name="2.2.1"></a>
## [2.2.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.2.0...2.2.1) (2020-12-08)

### Bug Fixes

* resolve [#135](https://gitlab.com/AdrianDC/gitlabci-local/issues/135): wrap colored strings and adapt boxes dimensions

### Cleanups

* prepare [#135](https://gitlab.com/AdrianDC/gitlabci-local/issues/135): isolate string manipulators to 'Strings' type

### Features

* resolve [#136](https://gitlab.com/AdrianDC/gitlabci-local/issues/136): adapt update hint to sudo-installed packages


<a name="2.2.0"></a>
## [2.2.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.1.2...2.2.0) (2020-12-07)

### Cleanups

* gitlab-ci: implement 'gitlab-release' to fill tags releases
* changelog: create a CHANGELOG version description extractor
* resolve [#134](https://gitlab.com/AdrianDC/gitlabci-local/issues/134): isolate environment variables inside 'Bundle'
* resolve [#134](https://gitlab.com/AdrianDC/gitlabci-local/issues/134): isolate package names to a 'Bundle' class
* prepare [#131](https://gitlab.com/AdrianDC/gitlabci-local/issues/131): add 'REPOSITORY' GitLab URL link constant
* implement [#133](https://gitlab.com/AdrianDC/gitlabci-local/issues/133): isolate all colors attributes into a class
* readme: add 'native' local jobs as supported engine

### Documentation

* prepare [#118](https://gitlab.com/AdrianDC/gitlabci-local/issues/118): add supported macOS versions and update TEST

### Features

* implement [#131](https://gitlab.com/AdrianDC/gitlabci-local/issues/131): refactor the updates message with hints
* prepare [#131](https://gitlab.com/AdrianDC/gitlabci-local/issues/131): create 'Boxes' class to create boxed messages
* implement [#133](https://gitlab.com/AdrianDC/gitlabci-local/issues/133): add 'center' and 'strip' string manipulators
* implement [#131](https://gitlab.com/AdrianDC/gitlabci-local/issues/131): check for updates without delay upon exit
* implement [#132](https://gitlab.com/AdrianDC/gitlabci-local/issues/132): use the original userspace if using sudo
* prepare [#132](https://gitlab.com/AdrianDC/gitlabci-local/issues/132): provide IS_USER_SUDO and USER_SUDO constants


<a name="2.1.2"></a>
## [2.1.2](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.1.1...2.1.2) (2020-12-05)

### Bug Fixes

* resolve [#130](https://gitlab.com/AdrianDC/gitlabci-local/issues/130): respect list selector single choice inputs
* resolve [#129](https://gitlab.com/AdrianDC/gitlabci-local/issues/129): import modules libraries before components

### Cleanups

* tests: add 'images' test job for native and container jobs
* types: refactor 'Dicts.find' without regex dependency
* readme: add command usage entrypoint and shortcuts table
* readme: drop the unreadable and old usage short help header
* types: turn 'Paths' class methods into static methods

### Documentation

* resolve [#129](https://gitlab.com/AdrianDC/gitlabci-local/issues/129): document the settings configurations and goals

### Features

* prepare [#129](https://gitlab.com/AdrianDC/gitlabci-local/issues/129): add '--settings' to show the path and contents


<a name="2.1.1"></a>
## [2.1.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.1.0...2.1.1) (2020-12-05)

### Bug Fixes

* prepare [#121](https://gitlab.com/AdrianDC/gitlabci-local/issues/121): isolate print flushes and allow only on TTY out
* resolve [#127](https://gitlab.com/AdrianDC/gitlabci-local/issues/127): evaluate host project directories correctly

### Cleanups

* vscode: disable terminal app insights telemetry
* vscode: ensure YAML use single quotes formatting
* vscode: add recommended VSCode extensions list
* vscode: always format files upon editor saves
* vscode: configure VSCode telemetry and privacy settings

### Documentation

* prepare [#118](https://gitlab.com/AdrianDC/gitlabci-local/issues/118): add macOS references in README and TEST

### Features

* implement [#128](https://gitlab.com/AdrianDC/gitlabci-local/issues/128): store and read default engines in settings
* resolve [#122](https://gitlab.com/AdrianDC/gitlabci-local/issues/122): add CI_JOB_NAME and CI_PROJECT_DIR definitions
* prepare [#122](https://gitlab.com/AdrianDC/gitlabci-local/issues/122): allow expanding CI_LOCAL in variables values
* prepare [#118](https://gitlab.com/AdrianDC/gitlabci-local/issues/118): support macOS paths, userspace and real paths
* prepare [#118](https://gitlab.com/AdrianDC/gitlabci-local/issues/118): add Platform.IS_MAC_OS platform detection
* prepare [#118](https://gitlab.com/AdrianDC/gitlabci-local/issues/118): restrict Docker sockets mounts to Linux only

### Test

* validate [#122](https://gitlab.com/AdrianDC/gitlabci-local/issues/122): create specific test cases for CI projects


<a name="2.1.0"></a>
## [2.1.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.0.1...2.1.0) (2020-12-03)

### Cleanups

* resolve [#123](https://gitlab.com/AdrianDC/gitlabci-local/issues/123): isolate into classes and lint the sources
* gitlab-ci: isolate pip install steps in 'before_script'
* prepare [#123](https://gitlab.com/AdrianDC/gitlabci-local/issues/123): import only required libraries in setup.py
* prepare [#123](https://gitlab.com/AdrianDC/gitlabci-local/issues/123): import only required libraries in preview.py
* gitlab-ci: add local 'Lint' job as a pylint wrapper
* gitlab-ci: disable pip updates warnings in relevant jobs
* gitlab-ci: turn the 'Codestyle' job into a CI check job
* gitlab-ci: quiet pip installation logs in 'deploy' jobs
* gitlab-ci: isolate local jobs under a 'development' stage
* gitlab-ci: isolate requirements and use built packages
* gitlab-ci: add '--force-reinstall' to pip reinstallations

### Features

* implement [#124](https://gitlab.com/AdrianDC/gitlabci-local/issues/124): add daily PyPI updates notifications
* implement [#125](https://gitlab.com/AdrianDC/gitlabci-local/issues/125): implement a settings storage class
* implement [#126](https://gitlab.com/AdrianDC/gitlabci-local/issues/126): add network mode support in Podman engine


<a name="2.0.1"></a>
## [2.0.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/2.0.0...2.0.1) (2020-12-01)

### Bug Fixes

* resolve [#116](https://gitlab.com/AdrianDC/gitlabci-local/issues/116): fix native scripts working directory access
* resolve [#114](https://gitlab.com/AdrianDC/gitlabci-local/issues/114): show default prioritized engines list in --help

### Cleanups

* readme: isolate Linux and Windows tables in chapters
* readme: minor missing line break in native context jobs
* gitlab-ci: use 'Deploy Trial' name to avoid 'Test' issues
* gitlab-ci: add 'Preview' wrapper job for 'docs/preview.py'
* resolve [#111](https://gitlab.com/AdrianDC/gitlabci-local/issues/111): improve '-p' pipeline documentation details
* resolve [#112](https://gitlab.com/AdrianDC/gitlabci-local/issues/112): prevent line break of 'Hyper-V' in engines
* resolve [#119](https://gitlab.com/AdrianDC/gitlabci-local/issues/119): avoid preparing volumes on native jobs
* resolve [#112](https://gitlab.com/AdrianDC/gitlabci-local/issues/112): prevent line breaks in the tables
* resolve [#111](https://gitlab.com/AdrianDC/gitlabci-local/issues/111): cleanup typos and improve --help details

### Documentation

* resolve [#117](https://gitlab.com/AdrianDC/gitlabci-local/issues/117): add usual examples of parameters
* resolve [#120](https://gitlab.com/AdrianDC/gitlabci-local/issues/120): refactor the supported .gitlab-ci.yml nodes
* readme: add Windows 10 1909 as being a supported system
* test: add tools and engines references for Linux and Windows

### Features

* resolve [#113](https://gitlab.com/AdrianDC/gitlabci-local/issues/113): standardize --tags values as "list,of,values"


<a name="2.0.0"></a>
## [2.0.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.3.1...2.0.0) (2020-11-30)

### Bug Fixes

* gitlab-ci: resolve "${PWD}" path usage with spaces in tests
* resolve [#110](https://gitlab.com/AdrianDC/gitlabci-local/issues/110): fix non-interactive menus and engine on Windows
* resolve [#105](https://gitlab.com/AdrianDC/gitlabci-local/issues/105): handle duplicated source paths on Windows too
* resolve [#107](https://gitlab.com/AdrianDC/gitlabci-local/issues/107): support working directory in local native jobs
* resolve [#106](https://gitlab.com/AdrianDC/gitlabci-local/issues/106): use required pure POSIX paths for workdir paths
* resolve [#109](https://gitlab.com/AdrianDC/gitlabci-local/issues/109): disallow real paths usage on Windows
* resolve [#106](https://gitlab.com/AdrianDC/gitlabci-local/issues/106): resolve relative workdir paths against options
* resolve [#105](https://gitlab.com/AdrianDC/gitlabci-local/issues/105): handle volumes duplicates and local overrides
* resolve [#106](https://gitlab.com/AdrianDC/gitlabci-local/issues/106): resolve relative paths against configuration
* resolve [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): support local script paths with spaces
* resolve [#105](https://gitlab.com/AdrianDC/gitlabci-local/issues/105): support mounting a path twice without overlaps
* resolve [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): use only /builds folder for entrypoint scripts
* resolve [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): use isolated temporary directory to avoid issues
* gitlab-ci: use real paths and bind sockets for development
* gitlab-ci: refactor, nested containers and Podman 3.6 to 3.9
* gitlab-ci: resolve "${PWD}" real path upon environment tests
* resolve [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): avoid using host '/tmp' with container processes
* resolve [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): bind temp directory to avoid Hyper-V share spams
* resolve [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): use 'sh' explicitly for local native scripts
* test [#102](https://gitlab.com/AdrianDC/gitlabci-local/issues/102): test if CI_LOCAL_ENGINE_NAME is defined twice
* resolve [#104](https://gitlab.com/AdrianDC/gitlabci-local/issues/104): configure and instantiate the engine only once
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): resolve 'local: workdir' absolute path in parser
* resolve [#102](https://gitlab.com/AdrianDC/gitlabci-local/issues/102): ensure CI_LOCAL_ENGINE_NAME is set for all jobs
* prepare [#103](https://gitlab.com/AdrianDC/gitlabci-local/issues/103): use hidden internal members in Engine classes
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): resolve workdir absolute path before using it
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): prepare Windows specific changes in resolvePath
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): exclude /var/run/docker.sock from Windows mounts
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): add IS_LINUX and IS_WINDOWS constants
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): remove the temporary script only after execution
* finish [#89](https://gitlab.com/AdrianDC/gitlabci-local/issues/89): minor comments typo fixes upon time evaluations
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): use PurePosixPath for internal container paths
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): use Linux newline endings in entrypoint scripts
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): migrate from os.path to pathlib Path items
* resolve [#95](https://gitlab.com/AdrianDC/gitlabci-local/issues/95): avoid opening the NamedTemporaryFile file twice
* resolve [#96](https://gitlab.com/AdrianDC/gitlabci-local/issues/96): support non-regex names like "C++" in inputs
* resolve [#98](https://gitlab.com/AdrianDC/gitlabci-local/issues/98): avoid running incomplete jobs in pipelines
* finish [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): ensure the entrypoint script is user accessible
* finish [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): add '--privileged' flag for Podman containers
* finish [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): avoid CI_LOCAL_ENGINE / CI_LOCAL_ENGINE_NAME loop
* resolve [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): avoid Python 3.7+ specific 'capture_output'
* finish [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): define CI_LOCAL_ENGINE and resolve Podman tests
* test [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): use extends rather than anchos to keeps variables
* resolve [#91](https://gitlab.com/AdrianDC/gitlabci-local/issues/91): fix parser support for empty variables
* resolve [#90](https://gitlab.com/AdrianDC/gitlabci-local/issues/90): fix regex searches of names upon --dump
* gitlab-ci: remove PATH to avoid issues with Docker-in-Docker
* gitlab-ci: add engines sources to the codestyle input files
* gitlab-ci: migrate to Docker-in-Docker (dind) 19.03.13
* implement [#83](https://gitlab.com/AdrianDC/gitlabci-local/issues/83): add support for 'variables:' usage in 'image:'
* prepare [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): add missing 'linux-headers' for the Podman test
* tests: resolve entrypoint i686 / x86_64 unreliable results
* resolve [#81](https://gitlab.com/AdrianDC/gitlabci-local/issues/81): avoid invoking Docker APIs if running local jobs

### Cleanups

* gitlab-ci: add Test PyPI uploader local manual job
* docs: refresh the preview GIF for the latest 2.0.0 release
* docs: use Docker engine by default and minor cleanups
* docs: drop 'gitlabci-local --help' command in the preview
* gitignore: exclude all .tmp.* entrypoint intermediate files
* gitlab-ci: add 'pwd' and 'mount' to all tests jobs
* gitlab-ci: use the Docker engine by default for development
* run: add 'run.sh' script for local development purposes
* gitlab-ci: avoid reinstalling upon local native tests
* gitlab-ci: resolve colored terminal outputs in 'Test'
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): isolate /builds and /tmp paths in const class
* main: use a global variable for '.gitlab-ci.yml' file name
* gitlab-ci: install production requirements then development
* gitlab-ci: add command headers for the 'Test' local job
* gitlab-ci: add 'git --name-status' after 'Codestyle' fixes
* gitlab-ci: add 'Test' local job to run unit tests suites
* resolve [#86](https://gitlab.com/AdrianDC/gitlabci-local/issues/86): hide irrelevant internal values from --dump
* development: install as 'sudoer' when using 'Development'
* gitlab-ci: ensure /usr/local/path is in PATH for all tests
* prepare [#82](https://gitlab.com/AdrianDC/gitlabci-local/issues/82): ensure Python 3 is explicitly used in 'Deploy'
* dev: add missing setuptools-scm development requirement
* prepare [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): reduce Docker specific references and add OCI
* prepare [#82](https://gitlab.com/AdrianDC/gitlabci-local/issues/82): ensure Python 3 is explicitly used in 'Build'

### Code Refactoring

* prepare [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): isolate the Docker engine as an abstract
* prepare [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): isolate Docker engine specific APIs

### Documentation

* readme: center operating systems and engines names tables
* gitlab-ci: use 'docs: changelog:' for changelog commits
* readme: improve readability of supported engines and systems
* readme: refresh 'gitlabci-local' usage and parameters lists
* document [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): add supported systems and engines in README

### Features

* resolve [#108](https://gitlab.com/AdrianDC/gitlabci-local/issues/108): define CI_LOCAL_ENGINE if engine option is set
* resolve [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): automate interactive winpty calls on Windows
* implement [#101](https://gitlab.com/AdrianDC/gitlabci-local/issues/101): add '-S' to manually mount engine sockets
* implement [#103](https://gitlab.com/AdrianDC/gitlabci-local/issues/103): see the used engine in the job header
* resolve [#100](https://gitlab.com/AdrianDC/gitlabci-local/issues/100): add '.local: real_paths:' configuration
* resolve [#100](https://gitlab.com/AdrianDC/gitlabci-local/issues/100): use /builds paths for the temporary script
* resolve [#100](https://gitlab.com/AdrianDC/gitlabci-local/issues/100): use /builds paths and add '-r' for real mounts
* resolve [#99](https://gitlab.com/AdrianDC/gitlabci-local/issues/99): add support and tests for Python 3.9.0
* resolve [#93](https://gitlab.com/AdrianDC/gitlabci-local/issues/93): add 'docker,' / 'podman,' for engines priority
* implement [#92](https://gitlab.com/AdrianDC/gitlabci-local/issues/92): add '.local:engine' default configurations
* finish [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): refactor with Podman subprocess CLI calls
* fix [#85](https://gitlab.com/AdrianDC/gitlabci-local/issues/85): resolve puller access to job options 'host'
* fix [#87](https://gitlab.com/AdrianDC/gitlabci-local/issues/87): use setuptools API for the --version informations
* extend [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): add -E engine selection and add CI_LOCAL_ENGINE
* implement [#89](https://gitlab.com/AdrianDC/gitlabci-local/issues/89): improve pipeline total duration outputs
* implement [#88](https://gitlab.com/AdrianDC/gitlabci-local/issues/88): add 'image: local:silent' as host silent jobs
* implement [#87](https://gitlab.com/AdrianDC/gitlabci-local/issues/87): add support for --version informations
* implement [#85](https://gitlab.com/AdrianDC/gitlabci-local/issues/85): add 'image: local:quiet' for host quiet jobs
* implement [#84](https://gitlab.com/AdrianDC/gitlabci-local/issues/84): accept -c with folder path to .gitlab-ci.yml
* implement [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): add Podman root / sudoers engine support
* finish [#79](https://gitlab.com/AdrianDC/gitlabci-local/issues/79): add 'Statistics' links for PyPI
* implement [#82](https://gitlab.com/AdrianDC/gitlabci-local/issues/82): add -H or --host to force host local usage

### Parser

* resolve [#94](https://gitlab.com/AdrianDC/gitlabci-local/issues/94): ignore and consider trigger jobs as disabled

### Test

* prepare [#105](https://gitlab.com/AdrianDC/gitlabci-local/issues/105): specific tests for local and CLI volumes
* prepare [#80](https://gitlab.com/AdrianDC/gitlabci-local/issues/80): add Podman specific test job for reference


<a name="1.3.1"></a>
## [1.3.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.3.0...1.3.1) (2020-10-23)

### Features

* resolve [#79](https://gitlab.com/AdrianDC/gitlabci-local/issues/79): add 'Bug Reports' and 'Source' links for PyPI
* implement [#78](https://gitlab.com/AdrianDC/gitlabci-local/issues/78): add total pipeline time in results


<a name="1.3.0"></a>
## [1.3.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.2.1...1.3.0) (2020-10-21)

### Bug Fixes

* resolve [#77](https://gitlab.com/AdrianDC/gitlabci-local/issues/77): resolve standalone multiline scripts parser

### Cleanups

* setup: add support for comments in requirements.txt
* requirements: bind setuptools for delivery rather than dev

### Features

* resolve [#74](https://gitlab.com/AdrianDC/gitlabci-local/issues/74): disable incomplete jobs instead of failing

### Test

* validate [#77](https://gitlab.com/AdrianDC/gitlabci-local/issues/77): check standalone multiline scripts parser


<a name="1.2.1"></a>
## [1.2.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.2.0...1.2.1) (2020-08-04)

### Bug Fixes

* resolve [#70](https://gitlab.com/AdrianDC/gitlabci-local/issues/70): support disabling *script: nodes with extends:
* resolve [#69](https://gitlab.com/AdrianDC/gitlabci-local/issues/69): propagate and cumulate extended jobs' variables
* resolve [#68](https://gitlab.com/AdrianDC/gitlabci-local/issues/68): add empty footer lines upon error failures

### Cleanups

* gitlab-ci: remove unnecessary 'tags: local' for local jobs

### Features

* implement [#73](https://gitlab.com/AdrianDC/gitlabci-local/issues/73): add support for regex searches of names
* resolve [#72](https://gitlab.com/AdrianDC/gitlabci-local/issues/72): add support for the --help parameter along -h
* document [#71](https://gitlab.com/AdrianDC/gitlabci-local/issues/71): add 'gcil' alias references in help and README
* implement [#71](https://gitlab.com/AdrianDC/gitlabci-local/issues/71): add a shorter "gcil" entrypoint wrapper
* implement [#67](https://gitlab.com/AdrianDC/gitlabci-local/issues/67): define CI_LOCAL variable to detect local jobs

### Test

* validate [#71](https://gitlab.com/AdrianDC/gitlabci-local/issues/71): check 'gcil' works on the 'simple' tests


<a name="1.2.0"></a>
## [1.2.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.1.6...1.2.0) (2020-06-13)

### Bug Fixes

* prepare [#66](https://gitlab.com/AdrianDC/gitlabci-local/issues/66): respect included data order in 'include' nodes
* prepare [#66](https://gitlab.com/AdrianDC/gitlabci-local/issues/66): ensure global keys will not be parsed as jobs
* prepare [#66](https://gitlab.com/AdrianDC/gitlabci-local/issues/66): ensure missing 'script' required node detection
* prepare [#66](https://gitlab.com/AdrianDC/gitlabci-local/issues/66): ensure missing 'image' key is properly detected

### Features

* implement [#66](https://gitlab.com/AdrianDC/gitlabci-local/issues/66): add support for 'extends' jobs in parser

### Test

* validate [#66](https://gitlab.com/AdrianDC/gitlabci-local/issues/66): ensure 'extends' full support is validated


<a name="1.1.6"></a>
## [1.1.6](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.1.5...1.1.6) (2020-04-02)

### Bug Fixes

* resolve [#65](https://gitlab.com/AdrianDC/gitlabci-local/issues/65): synchronize stdout and stderr runner outputs

### Cleanups

* validate [#64](https://gitlab.com/AdrianDC/gitlabci-local/issues/64): ensure first failure drops the script

### Features

* implement [#62](https://gitlab.com/AdrianDC/gitlabci-local/issues/62): add support for 'allow_failure: true' options
* implement [#63](https://gitlab.com/AdrianDC/gitlabci-local/issues/63): add execution timings for every job


<a name="1.1.5"></a>
## [1.1.5](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.1.4...1.1.5) (2020-03-15)

### Bug Fixes

* resolve UTF-8 stdout outputs from container logs stream

### Cleanups

* deprecate 'Deploy Test' and enforce automatic tags release


<a name="1.1.4"></a>
## [1.1.4](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.1.3...1.1.4) (2020-03-15)

### Bug Fixes

* fix [#61](https://gitlab.com/AdrianDC/gitlabci-local/issues/61): handle before_script and script together like CI


<a name="1.1.3"></a>
## [1.1.3](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.1.2...1.1.3) (2020-03-10)

### Bug Fixes

* implement [#61](https://gitlab.com/AdrianDC/gitlabci-local/issues/61): handle before_script and after_script like CI
* resolve Python codestyle with YAPF in parser and runner

### Cleanups

* add 'Dependencies' development requirements local job

### Features

* implement [#59](https://gitlab.com/AdrianDC/gitlabci-local/issues/59): add support for bash in debug mode
* implement [#60](https://gitlab.com/AdrianDC/gitlabci-local/issues/60): adapt debug command if bash exists


<a name="1.1.2"></a>
## [1.1.2](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.1.1...1.1.2) (2020-03-07)

### Bug Fixes

* tests: minor local test output syntax cleanup
* finish [#57](https://gitlab.com/AdrianDC/gitlabci-local/issues/57): ensure --debug works upon runner failures too


<a name="1.1.1"></a>
## [1.1.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.1.0...1.1.1) (2020-03-03)

### Features

* implement [#57](https://gitlab.com/AdrianDC/gitlabci-local/issues/57): add --debug support to keep runner execution
* implement [#58](https://gitlab.com/AdrianDC/gitlabci-local/issues/58): handle SIGTERM as an interruption


<a name="1.1.0"></a>
## [1.1.0](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.0.5...1.1.0) (2020-02-23)

### Bug Fixes

* resolve [#55](https://gitlab.com/AdrianDC/gitlabci-local/issues/55): use stable docker:19.03.5-dind image service
* resolve [#53](https://gitlab.com/AdrianDC/gitlabci-local/issues/53): parse complete context before parsing stages
* resolve [#51](https://gitlab.com/AdrianDC/gitlabci-local/issues/51): handle global variables as default values only
* resolve [#49](https://gitlab.com/AdrianDC/gitlabci-local/issues/49): preserve environment variables when set in .env

### Cleanups

* finish [#48](https://gitlab.com/AdrianDC/gitlabci-local/issues/48): add missing '.local:network' mention in README
* regenerate preview GIF documentation
* finish [#56](https://gitlab.com/AdrianDC/gitlabci-local/issues/56): cleanup supported .gitlab-ci.yml features
* refresh the README usage helper parameters list
* fix the README and helper tool name to 'gitlabci-local'
* finish [#54](https://gitlab.com/AdrianDC/gitlabci-local/issues/54): add missing tests/includes unit tests call
* resolve [#56](https://gitlab.com/AdrianDC/gitlabci-local/issues/56): document all supported .gitlab-ci.yml features
* finish [#47](https://gitlab.com/AdrianDC/gitlabci-local/issues/47): add '.local:env' mention in README.md
* refresh preview GIF for latest features and parameters
* remove unused configurations variable in parser.py
* ensure Unit Tests jobs timeout after 10 minutes
* resolve colored codestyle with YAPF

### Documentation

* regenerate preview GIF with latest changes for 'failures'

### Features

* add support for 'names' in .local node configurations
* add support for 'when:' result details for clarity
* study [#55](https://gitlab.com/AdrianDC/gitlabci-local/issues/55): add 'Unit Tests (PyPI)' manual customized job
* implement [#54](https://gitlab.com/AdrianDC/gitlabci-local/issues/54): initial support for include:local nodes
* resolve [#47](https://gitlab.com/AdrianDC/gitlabci-local/issues/47): add support for env parsing in .local node
* implement [#50](https://gitlab.com/AdrianDC/gitlabci-local/issues/50): always enable before/after_script by default
* resolve [#52](https://gitlab.com/AdrianDC/gitlabci-local/issues/52): expand volume paths containing variables
* implement [#48](https://gitlab.com/AdrianDC/gitlabci-local/issues/48): add support for a network mode configuration
* implement [#46](https://gitlab.com/AdrianDC/gitlabci-local/issues/46): implement most parameters in .local nodes


<a name="1.0.5"></a>
## [1.0.5](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.0.4...1.0.5) (2020-01-28)

### Bug Fixes

* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): migrate from Blessings to Colored library

### Cleanups

* changelog: add current commit hint with git describe
* prepare [#34](https://gitlab.com/AdrianDC/gitlabci-local/issues/34): add 'winpty' references for Windows in README
* resolve [#44](https://gitlab.com/AdrianDC/gitlabci-local/issues/44): restrict Python to versions 3.6, 3.7 and 3.8
* setup: add 'Documentation' reference to README.md
* prepare [#44](https://gitlab.com/AdrianDC/gitlabci-local/issues/44): add Python 3.6, 3.7, 3.8 and local tests
* requirements: rename _dev.txt to requirements-dev.txt
* docs: refactor preview.sh Executor class with constants
* tests: add --pull feature validation upon entrypoints test
* gitlab-ci: isolate local preparation jobs to prepare stage

### Features

* implement [#43](https://gitlab.com/AdrianDC/gitlabci-local/issues/43): allow enabling all jobs with --all
* implement [#41](https://gitlab.com/AdrianDC/gitlabci-local/issues/41): add support for local volumes definitions
* prepare [#41](https://gitlab.com/AdrianDC/gitlabci-local/issues/41): support overriding a bound volume with another
* prepare [#41](https://gitlab.com/AdrianDC/gitlabci-local/issues/41): add support for :ro and :rw volume mounts flags
* implement [#42](https://gitlab.com/AdrianDC/gitlabci-local/issues/42): disable configurations with --defaults
* implement [#40](https://gitlab.com/AdrianDC/gitlabci-local/issues/40): migrate to .local unified configurations node


<a name="1.0.4"></a>
## [1.0.4](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.0.3...1.0.4) (2020-01-26)

### Bug Fixes

* resolve [#4](https://gitlab.com/AdrianDC/gitlabci-local/issues/4): fix list view separator in PyInquirer
* resolve [#39](https://gitlab.com/AdrianDC/gitlabci-local/issues/39): resolve Docker Python random exceptions
* resolve [#36](https://gitlab.com/AdrianDC/gitlabci-local/issues/36): support overriding image entrypoint with none
* resolve [#31](https://gitlab.com/AdrianDC/gitlabci-local/issues/31): hardcode the README GIF preview with tags
* resolve [#36](https://gitlab.com/AdrianDC/gitlabci-local/issues/36): preserve original image and CI YAML entrypoints
* resolve [#33](https://gitlab.com/AdrianDC/gitlabci-local/issues/33) support integer variables definitiionz type
* resolve [#13](https://gitlab.com/AdrianDC/gitlabci-local/issues/13): fix rare container wait random failures

### Cleanups

* codestyle: pass all Python files through unify with "'"
* codestyle: pass all Python sources through YAPF
* codestyle: add an automated YAPF local job wrapper
* requirements: add YAPF as a development requirement
* requirements: unify and add missing developement items
* development: only rebuild in the Development local stage

### Features

* implement [#3](https://gitlab.com/AdrianDC/gitlabci-local/issues/3): support job retry values upon executions
* implement [#38](https://gitlab.com/AdrianDC/gitlabci-local/issues/38): pull Docker images if missing upon execution
* implement [#37](https://gitlab.com/AdrianDC/gitlabci-local/issues/37): use low-level Docker pull with streamed logs
* implement [#32](https://gitlab.com/AdrianDC/gitlabci-local/issues/32): add --pull mode for Docker images

### README

* resolve Changelog job reference for 'image: local'
* add pexpect references for docs/ automated preview script


<a name="1.0.3"></a>
## [1.0.3](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.0.2...1.0.3) (2020-01-23)

### Bug Fixes

* resolve [#26](https://gitlab.com/AdrianDC/gitlabci-local/issues/26): use .env variables only as default values
* fix [#25](https://gitlab.com/AdrianDC/gitlabci-local/issues/25): prevent tags parameters from appending default tags
* resolve [#21](https://gitlab.com/AdrianDC/gitlabci-local/issues/21): stop Docker container upon user interruption
* resolve [#17](https://gitlab.com/AdrianDC/gitlabci-local/issues/17): support user interruptions

### CHANGELOG

* implement [#20](https://gitlab.com/AdrianDC/gitlabci-local/issues/20): automate tag and log regeneration

### Cleanups

* resolve [#15](https://gitlab.com/AdrianDC/gitlabci-local/issues/15): document the .configurations features
* implement [#27](https://gitlab.com/AdrianDC/gitlabci-local/issues/27): add local build and test wrapper

### Features

* implement [#30](https://gitlab.com/AdrianDC/gitlabci-local/issues/30): add support for working directory parameter
* implement [#29](https://gitlab.com/AdrianDC/gitlabci-local/issues/29): add support for specific volume mounts
* implement [#28](https://gitlab.com/AdrianDC/gitlabci-local/issues/28): add support for specific environment files
* implement [#22](https://gitlab.com/AdrianDC/gitlabci-local/issues/22): add support for passing environment variables
* resolve [#25](https://gitlab.com/AdrianDC/gitlabci-local/issues/25): use listed values for -t tags parameters
* implement [#23](https://gitlab.com/AdrianDC/gitlabci-local/issues/23): add support for native local jobs execution
* implement [#19](https://gitlab.com/AdrianDC/gitlabci-local/issues/19): add support for YAML and JSON configurations
* implement [#16](https://gitlab.com/AdrianDC/gitlabci-local/issues/16): configure with environment variables if set
* implement [#18](https://gitlab.com/AdrianDC/gitlabci-local/issues/18): extend user configurations support for types

### README

* resolve [#24](https://gitlab.com/AdrianDC/gitlabci-local/issues/24): document special usage cases


<a name="1.0.2"></a>
## [1.0.2](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.0.1...1.0.2) (2020-01-21)

### Bug Fixes

* implement [#1](https://gitlab.com/AdrianDC/gitlabci-local/issues/1): add --manual-tags default values documentation
* resolve [#8](https://gitlab.com/AdrianDC/gitlabci-local/issues/8): ensure Docker and other dependencies are recent

### CHANGELOG

* implement [#11](https://gitlab.com/AdrianDC/gitlabci-local/issues/11): create initial CHANGELOG with git-chglog

### Cleanups

* resolve [#12](https://gitlab.com/AdrianDC/gitlabci-local/issues/12): apply VSCode, MarkdownLint and YAPF settings
* implement [#9](https://gitlab.com/AdrianDC/gitlabci-local/issues/9): unify dependencies under requirements.txt

### Documentation

* regenerate preview documentations and fix quotes

### Features

* implement [#11](https://gitlab.com/AdrianDC/gitlabci-local/issues/11): add Changelog link on PyPI releases
* implement [#10](https://gitlab.com/AdrianDC/gitlabci-local/issues/10): support local job tag as being manual jobs
* implement [#7](https://gitlab.com/AdrianDC/gitlabci-local/issues/7): load .env local environment variables
* resolve [#6](https://gitlab.com/AdrianDC/gitlabci-local/issues/6): allow menu selections while using --pipeline

### README

* resolve [#5](https://gitlab.com/AdrianDC/gitlabci-local/issues/5): add dependencies list and purposes


<a name="1.0.1"></a>
## [1.0.1](https://gitlab.com/AdrianDC/gitlabci-local/compare/1.0.0...1.0.1) (2020-01-20)

### Features

* implement [#2](https://gitlab.com/AdrianDC/gitlabci-local/issues/2): add .configurations dynamic user choices


<a name="1.0.0"></a>
## 1.0.0 (2020-01-18)

