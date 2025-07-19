package main

import (
	"context"
	"fmt"
	"log"
	"os/exec"
	"strings"
	"time"

	"github.com/hashicorp/packer-plugin-sdk/multistep"
	"github.com/hashicorp/packer-plugin-sdk/packer"
)

// stepCreateVM creates a new VM using Meda
type stepCreateVM struct{}

func (s *stepCreateVM) Run(ctx context.Context, state multistep.StateBag) multistep.StepAction {
	config := state.Get("config").(*Config)
	ui := state.Get("ui").(packer.Ui)
	vmName := state.Get("vm_name").(string)

	ui.Say(fmt.Sprintf("Creating VM '%s' with base image '%s'", vmName, config.BaseImage))

	var cmd *exec.Cmd
	if config.UseAPI {
		// Use REST API to create VM
		cmd = exec.Command("curl", "-X", "POST",
			fmt.Sprintf("http://%s:%d/api/v1/vms", config.MedaHost, config.MedaPort),
			"-H", "Content-Type: application/json",
			"-d", fmt.Sprintf(`{
				"name": "%s",
				"base_image": "%s",
				"memory": "%s",
				"cpus": %d,
				"disk": "%s",
				"force": false
			}`, vmName, config.BaseImage, config.Memory, config.CPUs, config.DiskSize))
	} else {
		// Use CLI to create VM
		args := []string{"run", config.BaseImage, "--name", vmName,
			"--memory", config.Memory,
			"--cpus", fmt.Sprintf("%d", config.CPUs),
			"--disk", config.DiskSize,
			"--no-start"}
		
		if config.UserDataFile != "" {
			args = append(args, "--user-data", config.UserDataFile)
		}
		
		// Use cargo run for development
		if config.MedaBinary == "cargo" {
			cargoArgs := append([]string{"run", "--"}, args...)
			cmd = exec.Command("cargo", cargoArgs...)
			cmd.Dir = "/home/ubuntu/meda" // Set working directory for cargo
		} else {
			cmd = exec.Command(config.MedaBinary, args...)
		}
	}

	output, err := cmd.CombinedOutput()
	if err != nil {
		err := fmt.Errorf("failed to create VM: %s - %s", err, string(output))
		state.Put("error", err)
		ui.Error(err.Error())
		return multistep.ActionHalt
	}

	ui.Say(fmt.Sprintf("VM '%s' created successfully", vmName))
	return multistep.ActionContinue
}

func (s *stepCreateVM) Cleanup(state multistep.StateBag) {
	// Cleanup will be handled by stepCleanupVM
}

// stepStartVM starts the VM
type stepStartVM struct{}

func (s *stepStartVM) Run(ctx context.Context, state multistep.StateBag) multistep.StepAction {
	config := state.Get("config").(*Config)
	ui := state.Get("ui").(packer.Ui)
	vmName := state.Get("vm_name").(string)

	ui.Say(fmt.Sprintf("Starting VM '%s'", vmName))

	var cmd *exec.Cmd
	if config.UseAPI {
		cmd = exec.Command("curl", "-X", "POST",
			fmt.Sprintf("http://%s:%d/api/v1/vms/%s/start", config.MedaHost, config.MedaPort, vmName))
	} else {
		if config.MedaBinary == "cargo" {
			cmd = exec.Command("cargo", "run", "--", "start", vmName)
			cmd.Dir = "/home/ubuntu/meda"
		} else {
			cmd = exec.Command(config.MedaBinary, "start", vmName)
		}
	}

	output, err := cmd.CombinedOutput()
	if err != nil {
		err := fmt.Errorf("failed to start VM: %s - %s", err, string(output))
		state.Put("error", err)
		ui.Error(err.Error())
		return multistep.ActionHalt
	}

	ui.Say(fmt.Sprintf("VM '%s' started successfully", vmName))
	return multistep.ActionContinue
}

func (s *stepStartVM) Cleanup(state multistep.StateBag) {}

// stepWaitForVM waits for the VM to be ready and gets its IP
type stepWaitForVM struct{}

func (s *stepWaitForVM) Run(ctx context.Context, state multistep.StateBag) multistep.StepAction {
	config := state.Get("config").(*Config)
	ui := state.Get("ui").(packer.Ui)
	vmName := state.Get("vm_name").(string)

	ui.Say(fmt.Sprintf("Waiting for VM '%s' to be ready...", vmName))

	// Wait for VM to be running and get IP
	timeout := time.After(5 * time.Minute)
	ticker := time.NewTicker(10 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-timeout:
			err := fmt.Errorf("timeout waiting for VM to be ready")
			state.Put("error", err)
			ui.Error(err.Error())
			return multistep.ActionHalt
		case <-ticker.C:
			var cmd *exec.Cmd
			if config.UseAPI {
				cmd = exec.Command("curl", "-s",
					fmt.Sprintf("http://%s:%d/api/v1/vms/%s/ip", config.MedaHost, config.MedaPort, vmName))
			} else {
				if config.MedaBinary == "cargo" {
					cmd = exec.Command("cargo", "run", "--", "ip", vmName)
					cmd.Dir = "/home/ubuntu/meda"
				} else {
					cmd = exec.Command(config.MedaBinary, "ip", vmName)
				}
			}

			output, err := cmd.CombinedOutput()
			if err == nil && len(output) > 0 {
				ip := strings.TrimSpace(string(output))
				if ip != "" && ip != "null" {
					state.Put("vm_ip", ip)
					// Set communicator host
					config.Comm.SSHHost = ip
					ui.Say(fmt.Sprintf("VM is ready with IP: %s", ip))
					return multistep.ActionContinue
				}
			}
			ui.Say("VM not ready yet, waiting...")
		}
	}
}

func (s *stepWaitForVM) Cleanup(state multistep.StateBag) {}

// stepStopVM stops the VM
type stepStopVM struct{}

func (s *stepStopVM) Run(ctx context.Context, state multistep.StateBag) multistep.StepAction {
	config := state.Get("config").(*Config)
	ui := state.Get("ui").(packer.Ui)
	vmName := state.Get("vm_name").(string)

	ui.Say(fmt.Sprintf("Stopping VM '%s'", vmName))

	var cmd *exec.Cmd
	if config.UseAPI {
		cmd = exec.Command("curl", "-X", "POST",
			fmt.Sprintf("http://%s:%d/api/v1/vms/%s/stop", config.MedaHost, config.MedaPort, vmName))
	} else {
		if config.MedaBinary == "cargo" {
			cmd = exec.Command("cargo", "run", "--", "stop", vmName)
			cmd.Dir = "/home/ubuntu/meda"
		} else {
			cmd = exec.Command(config.MedaBinary, "stop", vmName)
		}
	}

	output, err := cmd.CombinedOutput()
	if err != nil {
		log.Printf("Warning: failed to stop VM: %s - %s", err, string(output))
		// Continue anyway - VM might already be stopped
	} else {
		ui.Say(fmt.Sprintf("VM '%s' stopped successfully", vmName))
	}

	return multistep.ActionContinue
}

func (s *stepStopVM) Cleanup(state multistep.StateBag) {}

// stepCreateImage creates an image from the VM
type stepCreateImage struct{}

func (s *stepCreateImage) Run(ctx context.Context, state multistep.StateBag) multistep.StepAction {
	config := state.Get("config").(*Config)
	ui := state.Get("ui").(packer.Ui)
	vmName := state.Get("vm_name").(string)

	imageName := fmt.Sprintf("%s:%s", config.OutputImageName, config.OutputTag)
	ui.Say(fmt.Sprintf("Creating image '%s' from VM '%s'", imageName, vmName))

	var cmd *exec.Cmd
	if config.UseAPI {
		cmd = exec.Command("curl", "-X", "POST",
			fmt.Sprintf("http://%s:%d/api/v1/images", config.MedaHost, config.MedaPort),
			"-H", "Content-Type: application/json",
			"-d", fmt.Sprintf(`{
				"name": "%s",
				"tag": "%s",
				"from_vm": "%s"
			}`, config.OutputImageName, config.OutputTag, vmName))
	} else {
		if config.MedaBinary == "cargo" {
			cmd = exec.Command("cargo", "run", "--", "create-image", config.OutputImageName,
				"--tag", config.OutputTag,
				"--from-vm", vmName)
			cmd.Dir = "/home/ubuntu/meda"
		} else {
			cmd = exec.Command(config.MedaBinary, "create-image", config.OutputImageName,
				"--tag", config.OutputTag,
				"--from-vm", vmName)
		}
	}

	output, err := cmd.CombinedOutput()
	if err != nil {
		err := fmt.Errorf("failed to create image: %s - %s", err, string(output))
		state.Put("error", err)
		ui.Error(err.Error())
		return multistep.ActionHalt
	}

	state.Put("image_name", imageName)
	ui.Say(fmt.Sprintf("Image '%s' created successfully", imageName))
	return multistep.ActionContinue
}

func (s *stepCreateImage) Cleanup(state multistep.StateBag) {}

// stepCleanupVM cleans up the VM
type stepCleanupVM struct{}

func (s *stepCleanupVM) Run(ctx context.Context, state multistep.StateBag) multistep.StepAction {
	config := state.Get("config").(*Config)
	ui := state.Get("ui").(packer.Ui)
	vmName := state.Get("vm_name").(string)

	ui.Say(fmt.Sprintf("Cleaning up VM '%s'", vmName))

	var cmd *exec.Cmd
	if config.UseAPI {
		cmd = exec.Command("curl", "-X", "DELETE",
			fmt.Sprintf("http://%s:%d/api/v1/vms/%s", config.MedaHost, config.MedaPort, vmName))
	} else {
		if config.MedaBinary == "cargo" {
			cmd = exec.Command("cargo", "run", "--", "delete", vmName)
			cmd.Dir = "/home/ubuntu/meda"
		} else {
			cmd = exec.Command(config.MedaBinary, "delete", vmName)
		}
	}

	output, err := cmd.CombinedOutput()
	if err != nil {
		log.Printf("Warning: failed to delete VM: %s - %s", err, string(output))
		// Continue anyway - cleanup is best effort
	} else {
		ui.Say(fmt.Sprintf("VM '%s' cleaned up successfully", vmName))
	}

	return multistep.ActionContinue
}

func (s *stepCleanupVM) Cleanup(state multistep.StateBag) {
	// This is the cleanup step itself
}