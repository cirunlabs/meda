package main

import (
	"context"
	"fmt"
	"time"

	"github.com/hashicorp/hcl/v2/hcldec"
	"github.com/hashicorp/packer-plugin-sdk/multistep"
	"github.com/hashicorp/packer-plugin-sdk/multistep/commonsteps"
	"github.com/hashicorp/packer-plugin-sdk/packer"
)

const BuilderId = "meda.vm"

type Builder struct {
	config Config
	runner multistep.Runner
}

func (b *Builder) ConfigSpec() hcldec.ObjectSpec {
	return b.config.ConfigSpec()
}

func (b *Builder) Prepare(raws ...interface{}) (generatedVars []string, warnings []string, err error) {
	err = b.config.Prepare(raws...)
	if err != nil {
		return nil, nil, err
	}

	generatedVars = []string{
		"MedaVMName",
		"MedaVMIP",
	}

	return generatedVars, nil, nil
}

func (b *Builder) Run(ctx context.Context, ui packer.Ui, hook packer.Hook) (packer.Artifact, error) {
	// Set up the state
	state := new(multistep.BasicStateBag)
	state.Put("config", &b.config)
	state.Put("hook", hook)
	state.Put("ui", ui)

	// Generate unique VM name
	vmName := fmt.Sprintf("packer-%s-%d", b.config.VMName, time.Now().Unix())
	state.Put("vm_name", vmName)

	// Build the steps
	steps := []multistep.Step{
		&stepCreateVM{},
		&stepStartVM{},
		&stepWaitForVM{},
		&commonsteps.StepProvision{},
		&stepStopVM{},
		&stepCreateImage{},
		&stepPushImage{},
		&stepCleanupVM{},
	}

	// Setup the state bag and initial state for the steps
	b.runner = commonsteps.NewRunner(steps, b.config.PackerConfig, ui)
	b.runner.Run(ctx, state)

	// If there was an error, return that
	if rawErr, ok := state.GetOk("error"); ok {
		return nil, rawErr.(error)
	}

	// If we were interrupted or cancelled, then just exit.
	if _, ok := state.GetOk(multistep.StateCancelled); ok {
		return nil, fmt.Errorf("build was cancelled")
	}

	if _, ok := state.GetOk(multistep.StateHalted); ok {
		return nil, fmt.Errorf("build was halted")
	}

	// Get the image name from state
	imageName, ok := state.GetOk("image_name")
	if !ok {
		return nil, fmt.Errorf("failed to get image name from state")
	}

	// Get the pushed image name if it exists
	pushedImage, _ := state.GetOk("pushed_image")
	var pushedImageStr string
	if pushedImage != nil {
		pushedImageStr = pushedImage.(string)
	}

	artifact := &Artifact{
		ImageName:   imageName.(string),
		PushedImage: pushedImageStr,
		Config:      &b.config,
	}

	return artifact, nil
}

// GeneratedVars returns a list of variables that this builder generates
func (b *Builder) GeneratedVars() []string {
	return []string{
		"MedaVMName",
		"MedaVMIP",
	}
}