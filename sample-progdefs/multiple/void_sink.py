from common import InStream, Job, eprint, blastoff
from dataclasses import dataclass


@dataclass
class Inputs:
    sink: InStream


@dataclass
class Outputs:
    pass


async def run(job: Job) -> None:
    inputs = Inputs(**job.inputs)
    _ = Outputs(**job.outputs)
    i = 0
    async for message in inputs.sink.iterator():
        await message.ack()
        s = message.body.decode("utf-8")
        eprint(i, s)
        i += 1


if __name__ == "__main__":
    eprint("Void Sink Starting..")
    blastoff(run, 10_000)
