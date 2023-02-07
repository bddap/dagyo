import common
from common import asyncmain, InStream, Job, eprint
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
    async for _ in inputs.sink.iterator():
        eprint("dropping", i)


@asyncmain
async def main() -> None:
    eprint("Void Sink Starting")
    async for job in common.jobs():
        async with job:
            await run(job)
