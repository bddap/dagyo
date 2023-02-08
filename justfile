# https://github.com/casey/just

# These recipies will be removed once the sheduler is working
# they are currently being used for bootstrapping the scheduler.

AMPQ_PORT := "5672"
AMPQ_HOSTNAME := "dagyorabbitmq"
GREET_JOBS := "greetfakejobid"
SOURCE_JOBS := "sourcefakejobid"

default:
    just -l

# use the scheduler to run an example flow using the sample-progdefs
run-sample:
    #!/usr/bin/env bash
    set -ueo pipefail
    
    export DAGYO_VERTS="./sample-progdefs/vertspec.toml"
    export DAGYO_LOCAL="true"
    cargo run

start-local-cluster:
    minikube start --container-runtime=containerd

