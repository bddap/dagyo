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

## Dagyo Flow

A running data flow tree. Resources allocated to the flow include Daggo Executors and job 

## Dagyo Job

A runnable instance of a progdef. Dagyo Job are serialized and stored in a queue that Executors pull from. A job describes where to pull inputs, and where to push outputs.

In addition to the custom outputs specified by a job's Progdef. Jobs are provided with two additional outputs:
- "panic" - indicates the job has failed, by default the Dagyo Sheduler will kill and cleanup the entire flow.
- "health" - indicates the job is still running
   Each executor regulary pushes timestamps to this stream. The timestamps represent the aboslute
   time in milliseconds when this the job should be considered dead. If the Dagyo Scheduler 
   notices that a jobs most recently pushed timestamp is in the past, it may kill and cleanup the
   entire flow. Timestamps are encoded as ascii integers representing the nubmer of milliseconds since
   the unix epoch.

In addition to the custom inputs specified by a job's Progdef. Jobs are provided with one additional input:
- "stop" - if a job receives a message on this stream, it should stop this job and cleanup.

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

# Plans to Publish This Repository

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

[Kubeflow](https://www.kubeflow.org/docs) claims to be specifically for ML but maybe is general enough to use in place of dagyo or as a layer under dagyo.
Warrants further investigation. Some things to check:
- Can it stream?
- Can it typecheck?
- Can it scale?
- Can we use it to run user-defined flows?

# what sort of message broker do we need?

- Serialization: as long as messages can be byte arrays, that will be enough.
- We need the ability for an executor to pull exactly one job such that no other executor can pull the same message. This pattern is called [Competing Consumers](https://www.enterpriseintegrationpatterns.com/patterns/messaging/CompetingConsumers.html).
- Need strict ordering for of messages in a given stream. Don't need strict ordering across streams.
- Need to be able to delete mailboxes on cleanup.

RabbitMQ has nice tutorials: https://www.rabbitmq.com/getstarted.html

## Planned Upgrade Strategy:

Later we can add lifecycle hooks to the container definitions that tell progdefs to stop taking new jobs
then we'll allocate a large termination grace period, on the order of days, to allow any in-progress jobs to complete
before the pod shuts down.

This method should handle auto-scaledowns too. When a pod is being scaled down, the signall will tell it to stop taking new jobs
due to the large termination grace period, the pod will be allowed to finish any in-progress jobs before it shuts down.

After an upgrade, how do we ensure jobs in the out-of-date job queues are all processed?

## Plan for secrets management

We'll define a map from Progdef name to a enviroment dictionary. That list will go into the clusters configmap.
We'll then set each container's `env_from` setting to pull from configmap.

## Compute Resource Requirements

Progdef metadata will eventually include a field for compute resource requirements.
We'll use that to set the `resources` field on the container definition.

## Plan for Failure Tolerace

Failures, AKA "Panics", are aggregated for an entire flow. If one vert fails, the entire flow fails.
This leave the decision of what to do next to the user. Progdefs should be designed to minimize the
chance of flow-stopping failures. Here are some tips for reducing Panics.

Progdefs that perform IO should use retries when appicable and helpful.

Effective Progdefs with recoverable failure modes should have
with explicit failure modes as part of the defined interface.
For example, when a transformation on some data
might fail, the output type should be Result<T>, not T.
This output can be piped into, for exaple, an `assert` vert, or an `ignore_errors` vert
or a `log_errors` vert.

Another potential pattern could be to define an explicit "error" output stream for the progdef.
This may be a better fit for some circumstances, such as when an progdef has multiple outputs and
it would be unclear on which stream to report the error.

Of course, when there is an actual show-stopping failure, the flow *should* fail. An
example of a show-stopping failure is when a progdef with a defined interface recieves
a value that fails to deserialize. Deserialization errors on specified types should
be treated as panic-worthy bugs.

## Pod Preemption

https://kubernetes.io/docs/concepts/scheduling-eviction/pod-priority-preemption/

Flows are not recoverable so we never want to kill an executor while it's
running a job. We will be setting a high value for the
[gracefully termination period](https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-termination)
of executor pods, so pre-emption would not work against these pods in the first place.

## Recovery

Once popped from the queue, the state of a dagyo job exist in *only* one place: the
memory of the executor that is processing the job. This is an intentional design choice.

The ordering of elements within dagyo streams *is* respected.
Additionally dagyo ensures that not job is processed twice.
Each job is processed by at most one executor.
Executors' statefullnes is respected; dagyo will never swap out an executor mid-job.

In an alternate universe. Resumable executors were implemented by tracking the state each
job. This substantially complicated things for progdef writers. Progdefs needed to be
be written as state machines with serializable state. This paradigm is sometimes referred to
as "Checkpoint and Restore". While a nice set of abstractions
might have made Checkpoint and Restore less of a burden for Progdef implementers,
the version of daggo existing in our universe chooses not to spend any
[strangeness budget](https://steveklabnik.com/writing/the-language-strangeness-budget)
on the issue.

See also: section 3.4 "Fault tolerance and availability" of
[Naiad](https://www.microsoft.com/en-us/research/wp-content/uploads/2013/11/naiad_sosp2013.pdf)
for more thoughts on "Checkpoint and Restore".

This design choice is intentional, but you are welcome to get in touch if you have
ideas for implementing recoverable jobs. There may be a way to do this without putting
too much burden on progdef implementers.

## Misc Things Dagyo Needs

- [x] Type Checking
- [x] Example Progdefs
- [x] local demo
- [ ] remote demo.
- [x] Speedup image builds for faster feedback cycles.
- [ ] Generic types and type inference.
- [ ] Autoscaling, vertical and horizontal.
- [ ] A python library to simplify writing progdefs.
- [ ] Test suite that runs pipelines locally.
- [ ] Machine specs that let a progdef specify the type of machine it needs i.e. GPU.
- [ ] A way to feed a static input to a node. Ideally as bytes. Perhaps a special `const` node that plugs into a progdef.
- [ ] A way to extract the output from a flow as streams of bytes.
- [ ] Separate image build from kube deployment.
- [ ] Separate sheduler binary that runs *inside* the cluster.
- [ ] Scheduler needs to cleanup successful or failed flows.
- [ ] Sheduler to implement flow "panics".
- [ ] Progdef library to implement watching for "stop".
- [ ] Progdef library to implement "health" logic.
- [ ] Progdef library to stop accepting new jobs when kubernetes sends a termination.
- [ ] Let users pipe from "panic" endpoints?
- [ ] Let users pipe into "stop" endpoints?
- [ ] Internally consistent and precise vocabulary for describing parts of dagyo e.g. program vs. procedure.
- [ ] Ensure progdefs that crash without panicking do always cause the flow to panic.

