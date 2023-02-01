FROM python:3.10

RUN mkdir /app
WORKDIR /app

COPY pyproject.toml .
COPY poetry.lock .
RUN pip3 install poetry \
	&& poetry config virtualenvs.create false \
	&& poetry install --no-dev

COPY . .

ENTRYPOINT ["python", "./greet.py"]
