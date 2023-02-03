# https://github.com/casey/just

# These recipies will be removed once the sheduler is working
# they are currently being used for bootstrapping the scheduler.

AMPQ_PORT := "5672"
AMPQ_HOSTNAME := "dagyorabbitmq"
GREET_JOBS := "greetfakejobid"
SOURCE_JOBS := "sourcefakejobid"

default:
    just -l

rabbitmq:
    docker run --name {{ AMPQ_HOSTNAME }} -p {{ AMPQ_PORT }}:5672 --rm rabbitmq

greet:
    #!/usr/bin/env bash
    set -ueo pipefail

    cd sample-progdefs/multiple

    dbuild_args="--build-arg SCRIPT=./greet.py --file all.dockerfile ."

    docker build $dbuild_args
    docker run \
        -e DAGYO_JOBS={{ GREET_JOBS }} \
        -e DAGYO_QUEUE="amqp://guest:guest@{{ AMPQ_HOSTNAME }}:{{ AMPQ_PORT }}/" \
        --link {{ AMPQ_HOSTNAME }}:{{ AMPQ_HOSTNAME }} \
        --rm $(docker build -q $dbuild_args)

source:
    #!/usr/bin/env bash
    set -ueo pipefail

    cd sample-progdefs/multiple

    dbuild_args="--build-arg SCRIPT=./source.py --file all.dockerfile ."

    docker build $dbuild_args
    docker run \
        -e DAGYO_JOBS={{ SOURCE_JOBS }} \
        -e DAGYO_QUEUE="amqp://guest:guest@{{ AMPQ_HOSTNAME }}:{{ AMPQ_PORT }}/" \
        --link {{ AMPQ_HOSTNAME }}:{{ AMPQ_HOSTNAME }} \
        --rm $(docker build -q $dbuild_args)

# use the scheduler to run an example flow using the sample-progdefs
run-sample:
    #!/usr/bin/env bash
    set -ueo pipefail
    
    export DAGYO_QUEUE="amqp://guest:guest@{{ AMPQ_HOSTNAME }}:{{ AMPQ_PORT }}/"
    export DAGYO_VERTS="./sample-progdefs/vertspec.toml"
    cargo run

start-local-cluster:
    minikube start --container-runtime=containerd

