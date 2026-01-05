.PHONY: start-alloy stop-alloy test-telemetry clean-logs

# Start Grafana Alloy container for telemetry testing
start-alloy:
	docker run -d --name trix-alloy \
		-v ./config.alloy:/etc/alloy/config.alloy \
		-p 4318:4318 -p 12345:12345 \
		grafana/alloy:latest \
		run --server.http.listen-addr=0.0.0.0:12345 --storage.path=/var/lib/alloy/data --stability.level=experimental \
		/etc/alloy/config.alloy
	@echo "Alloy container started. Logs available with: docker logs -f trix-alloy"
	@echo "Web UI available at: http://localhost:12345"

# Stop and remove Grafana Alloy container
stop-alloy:
	docker stop trix-alloy && docker rm trix-alloy
	@echo "Alloy container stopped and removed."

# Run telemetry test with automatic cleanup
test-telemetry: start-alloy
	@echo "Waiting for Alloy to start..."
	@sleep 3
	@echo "Testing trix telemetry (make sure trix is configured to send to http://localhost:4318)..."
	@echo "Example: trix build --help"
	@echo "Check telemetry output with: docker logs -f trix-alloy"
	@echo "Press Enter to stop Alloy and cleanup..."
	@read _ && $(MAKE) stop-alloy

# Show Alloy logs for debugging
logs:
	docker logs -f trix-alloy

# Clean up any existing containers
clean:
	docker stop trix-alloy 2>/dev/null || true && docker rm trix-alloy 2>/dev/null || true
	@echo "Cleaned up existing containers."

# Help target
help:
	@echo "Available targets:"
	@echo "  start-alloy    - Start Grafana Alloy container"
	@echo "  stop-alloy     - Stop and remove container"
	@echo "  test-telemetry - Run telemetry test with automatic cleanup"
	@echo "  logs           - Follow Alloy container logs"
	@echo "  clean          - Clean up existing containers"
	@echo "  help           - Show this help message"