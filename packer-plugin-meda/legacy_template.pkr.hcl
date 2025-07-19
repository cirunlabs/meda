# Legacy JSON template for testing
source "null" "test" {
  communicator = "none"
}

build {
  sources = ["source.null.test"]
  
  provisioner "shell-local" {
    inline = [
      "echo 'Testing with null builder'",
      "echo 'Meda API available at: http://127.0.0.1:7777/api/v1/health'",
      "curl -s http://127.0.0.1:7777/api/v1/health"
    ]
  }
}