import common
from common import eprint, OutStream, InStream, blastoff
from dataclasses import dataclass


@dataclass
class Inputs:
    name: InStream


@dataclass
class Outputs:
    greeting: OutStream


async def run(job: common.Job) -> None:
    inputs = Inputs(**job.inputs)
    outputs = Outputs(**job.outputs)
    async for message in inputs.name:
        # convert message to a string, panic if it's not utf-8
        name = message.decode("utf-8")
        eprint("got name", name)
        await outputs.greeting.send(f"hello {name}".encode("utf-8"))


if __name__ == "__main__":
    eprint("starting greet.py")
    blastoff(run, 10_000)
