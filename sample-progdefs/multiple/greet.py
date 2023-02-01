import common
from common import asyncmain, eprint


@asyncmain
async def main() -> None:
    async for job in common.jobs():
        eprint(job)
