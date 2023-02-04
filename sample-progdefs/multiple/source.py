import common
from common import asyncmain, OutStream, Job, eprint
from dataclasses import dataclass

NAMES = [
    "Alice",
    "Bob",
    "Charlie",
    "David",
    "Eve",
    "Frank",
    "Grace",
    "Heidi",
    "Ivan",
    "Judy",
    "Karl",
    "Linda",
    "Mike",
    "Nancy",
    "Oscar",
    "Peggy",
    "Quinn",
    "Ruth",
    "Steve",
    "Tina",
    "Ursula",
    "Victor",
    "Wendy",
    "Xavier",
    "Yvonne",
    "Zach",
]


@dataclass
class Inputs:
    pass


@dataclass
class Outputs:
    some_strings: OutStream


async def run(job: Job) -> None:
    _ = Inputs(**job.inputs)
    outputs = Outputs(**job.outputs)
    for name in NAMES:
        await outputs.some_strings.send(name.encode("utf-8"))


@asyncmain
async def main() -> None:
    eprint("Source Starting")
    async for job in common.jobs():
        async with job:
            await run(job)
