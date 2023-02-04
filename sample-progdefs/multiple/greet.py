import common
from common import asyncmain, eprint
import time


@asyncmain
async def main() -> None:
    eprint("Greet Starting..")
    async for job in common.jobs():
        eprint(job)
