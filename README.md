# Definitions

## Dagyo

Dagyo is a distributed protocol for runing user-constructed data flow trees.

## Data Flow Tree

A DAG (Directed Acyclic Graph) where vertices are dagyo programs and edges data streams.

## Dagyo Progdef (Program Definition)

Two peices of data define a dagyo program:
- A Docker image definition
- Json Metadata describing:
  - A unique, human-readable name.
  - The progdef's inputs and outputs, including their types.
    - Each progdef has a constant number of inputs and outputs.
	- Each input an output is linked to a stream at runtime. Every stream specifies a Typename.
	- Each input and output has a human-readable name.
    - Input and outputs are unordered.
  - the progdef's execution environment, including its resource requirements e.g. accelerators.
  - The progdef's scaling behavior e.g. how many vertices can the progdef run at a time. Some lightwieght progdefs may be able to handle 1000s of invocations, while others may only be able to handle 1.
  - A relative path to the the dockerfile for this progdef.
  - healthcheck-timeout

All progdefs have two implicit outputs (in addition to the ones specified in metadata):
- "dagyo:panic" - indicates the job has failed, by default the Dagyo Sheduler will kill and cleanup the entire flow.
- "dagyo:healthy" - indicates the job is still running, if a job does not output to this stream for the specified period, the Dagyo Scheduler will kill and cleanup the entire flow.

Progdefs have one implicit input (in addition to the ones specified in metadata):
- "dagyo:stop" - if a job receives a message on this stream, it should stop this job and cleanup.

## Dagyo Flow

A running data flow tree. Resources allocated to the flow include Daggo Executors and job 

## Dagyo Job

A runnable instance of a progdef. Dagyo Job are serialized and stored in a queue that Executors pull from. A job describes where to pull inputs, and where to push outputs.

## Dagyo Executor

A runner of Dagyo Jobs. Each executor services exactly one progdef, but may run multiple Jobs at a time, depending on the progdef's specified scaling behavior.

## Dagyo Sheduler

A program that takes data flow trees and spawns dagyo workers to execute them. The sheduler is responsible for:
- Linking workers together.
- Cleaning up flows that have been completed execution.
  - This involves scaling down workers and cleaning up message queues.
- Cleaning up flows that have failed execution or have workers that did time out.

## Typename

A string representing the data that is carried over a stream. Every typename defines its own serialization. An edge from one progdef's output and another progdef's input is valid if and only if the two typenames are equal.

Each typename *should* have human readable documentation which describes the type and serialization.

Each type needs to be convertable to and from an array of octets.

# Implementing Custom Dagyo Executors

Executors are defined using Docker, and described using a JSON file.

When a Executor starts up, it reads the `DAGYO_QUEUE` environment variable and connects to a message queue at that address. The `DAGYO_JOBS` environment variable tells the executor which mailbox to pull jobs from.

When an deserialization error occurs, the dagyo executor should fail the job immediately by pushing to the failure queue.

# Msc.

https://blog.containerize.com/2021/07/09/top-5-open-source-message-queue-software-in-2021/

# Plans For This Repository

Dagyo protocol and scheduler should be open sourced under the a permissive license. Need to get formal permission for this.
This repo will contain example dagyo programs, but nothing specific to Postera.

# Why not X tool?

[Benthos](https://github.com/benthosdev/benthos): we need dynamic, user-defined data flow trees, not static pipelines.

[Apache Beam](https://beam.apache.org/): we need dynamic, user-defined data flow trees, not static pipelines.

[Apache Kafka](https://kafka.apache.org/): we need dynamic, user-defined data flow trees, not static pipelines. We don't need durability.

[Apache Flink](https://flink.apache.org/): we need dynamic, user-defined data flow trees, not static pipelines.

[Apache Airflow](https://airflow.apache.org/): Airflow is too expressive. It would grant the author of a data flow tree ability to run arbitrary code. Airflow is not itended for streaming.

Why not use aws lambda to run executors?
Timeouts. No support for streaming. No support for GPU acceleration. No support for binary serialization. Extra vendor lock-in.

# What sort of message broker do we need?

- Serialization: as long as messages can be byte arrays, that will be enough.
- We need the ability for an executor to pull exactly one job such that no other executor can pull the same message. FIFO is not strictly needed for job queue, but any message broker claiming to support FIFO should work.
- Need strict ordering for of messages in a given stream. Don't need strict ordering across streams.
- Need to be able to delete mailboxes on cleanup.

