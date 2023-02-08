from common import OutStream, Job, eprint, blastoff
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
    src: OutStream


async def run(job: Job) -> None:
    eprint("starting a job")
    _ = Inputs(**job.inputs)
    outputs = Outputs(**job.outputs)
    for name in NAMES:
        eprint("outputting", name)
        await outputs.src.send(name.encode("utf-8"))
    eprint("done with job")


if __name__ == "__main__":
    eprint("Source Starting..")
    blastoff(run, 10_000)
