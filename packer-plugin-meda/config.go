//go:generate packer-sdc mapstructure-to-hcl2 -type Config

package main

import (
	"fmt"
	"os"
	"os/exec"
	"time"

	"github.com/hashicorp/hcl/v2/hcldec"
	"github.com/hashicorp/packer-plugin-sdk/common"
	"github.com/hashicorp/packer-plugin-sdk/communicator"
	"github.com/hashicorp/packer-plugin-sdk/template/config"
	"github.com/hashicorp/packer-plugin-sdk/template/interpolate"
)

type Config struct {
	common.PackerConfig `mapstructure:",squash"`
	Comm                communicator.Config `mapstructure:",squash"`

	// Meda configuration
	MedaBinary   string `mapstructure:"meda_binary"`
	MedaHost     string `mapstructure:"meda_host"`
	MedaPort     int    `mapstructure:"meda_port"`
	UseAPI       bool   `mapstructure:"use_api"`

	// VM configuration
	VMName       string `mapstructure:"vm_name" required:"true"`
	BaseImage    string `mapstructure:"base_image" required:"true"`
	Memory       string `mapstructure:"memory"`
	CPUs         int    `mapstructure:"cpus"`
	DiskSize     string `mapstructure:"disk_size"`
	UserDataFile string `mapstructure:"user_data_file"`

	// Image output configuration
	OutputImageName string `mapstructure:"output_image_name" required:"true"`
	OutputTag       string `mapstructure:"output_tag"`
	Registry        string `mapstructure:"registry"`
	Organization    string `mapstructure:"organization"`

	// Push configuration
	PushToRegistry  bool   `mapstructure:"push_to_registry"`
	DryRun          bool   `mapstructure:"dry_run"`

	ctx interpolate.Context
}

func (c *Config) ConfigSpec() hcldec.ObjectSpec {
	return c.FlatMapstructure().HCL2Spec()
}

func (c *Config) Prepare(raws ...interface{}) error {
	err := config.Decode(c, &config.DecodeOpts{
		Interpolate:        true,
		InterpolateContext: &c.ctx,
		InterpolateFilter: &interpolate.RenderFilter{
			Exclude: []string{},
		},
	}, raws...)
	if err != nil {
		return err
	}

	// Set defaults
	if c.MedaBinary == "" {
		c.MedaBinary = "meda"
	}
	if c.MedaHost == "" {
		c.MedaHost = "127.0.0.1"
	}
	if c.MedaPort == 0 {
		c.MedaPort = 7777
	}
	if c.Memory == "" {
		c.Memory = "1G"
	}
	if c.CPUs == 0 {
		c.CPUs = 2
	}
	if c.DiskSize == "" {
		c.DiskSize = "10G"
	}
	if c.OutputTag == "" {
		c.OutputTag = "latest"
	}
	if c.Registry == "" {
		c.Registry = "ghcr.io"
	}

	// Validation
	var errs []error

	if c.VMName == "" {
		errs = append(errs, fmt.Errorf("vm_name is required"))
	}

	if c.BaseImage == "" {
		errs = append(errs, fmt.Errorf("base_image is required"))
	}

	if c.OutputImageName == "" {
		errs = append(errs, fmt.Errorf("output_image_name is required"))
	}

	// Check if meda binary exists if not using API
	if !c.UseAPI {
		if _, err := os.Stat(c.MedaBinary); os.IsNotExist(err) {
			// Try to find meda in PATH
			if _, err := exec.LookPath(c.MedaBinary); err != nil {
				errs = append(errs, fmt.Errorf("meda binary not found: %s", c.MedaBinary))
			}
		}
	}

	// Set up communicator defaults
	if c.Comm.Type == "" {
		c.Comm.Type = "ssh"
	}
	if c.Comm.SSHPort == 0 {
		c.Comm.SSHPort = 22
	}
	if c.Comm.SSHUsername == "" {
		c.Comm.SSHUsername = "cirun"
	}
	if c.Comm.SSHTimeout == 0 {
		c.Comm.SSHTimeout = 5 * time.Minute
	}
	if c.Comm.SSHPassword == "" {
		// Set a default password for Meda images
		c.Comm.SSHPassword = "cirun"
	}

	// SSH configuration for development
	c.Comm.SSHHandshakeAttempts = 10
	c.Comm.SSHDisableAgentForwarding = true

	// SSH host will be set dynamically in the step

	if len(errs) > 0 {
		return fmt.Errorf("validation errors: %v", errs)
	}

	return nil
}