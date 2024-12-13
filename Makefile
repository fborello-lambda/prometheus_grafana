DOCKER_COMPOSE = docker compose
DOCKER_COMPOSE_FILE = docker-compose.yaml

# Default target: build and run the application
.PHONY: all
all: build up

# Build the Docker image
.PHONY: build
build:
	$(DOCKER_COMPOSE) -f $(DOCKER_COMPOSE_FILE) build

# Start the application in detached mode
.PHONY: up
up:
	$(DOCKER_COMPOSE) -f $(DOCKER_COMPOSE_FILE) up -d

# Stop and remove containers
.PHONY: down
down:
	$(DOCKER_COMPOSE) -f $(DOCKER_COMPOSE_FILE) down

# Clean up Docker images and containers
.PHONY: clean
clean:
	$(DOCKER_COMPOSE) -f $(DOCKER_COMPOSE_FILE) down --rmi all --volumes
