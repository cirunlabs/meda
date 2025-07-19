package main

import (
	"fmt"
	"os/exec"
)

// Artifact represents the result of a Meda build
type Artifact struct {
	ImageName   string
	PushedImage string
	Config      *Config
}

// BuilderId returns the ID of the builder that created this artifact
func (a *Artifact) BuilderId() string {
	return BuilderId
}

// Files returns the files represented by this artifact
func (a *Artifact) Files() []string {
	// For Meda images, files are managed internally
	return nil
}

// Id returns the unique identifier for this artifact
func (a *Artifact) Id() string {
	return a.ImageName
}

// String returns a human-readable representation of this artifact
func (a *Artifact) String() string {
	if a.PushedImage != "" {
		return fmt.Sprintf("Meda image: %s (pushed to %s)", a.ImageName, a.PushedImage)
	}
	return fmt.Sprintf("Meda image: %s", a.ImageName)
}

// State returns the state data for this artifact
func (a *Artifact) State(name string) interface{} {
	switch name {
	case "image_name":
		return a.ImageName
	case "pushed_image":
		return a.PushedImage
	case "registry":
		return a.Config.Registry
	case "organization":
		return a.Config.Organization
	}
	return nil
}

// Destroy removes the artifact
func (a *Artifact) Destroy() error {
	// Use Meda to remove the image
	var cmd []string
	if a.Config.UseAPI {
		// API call to delete image
		cmd = []string{"curl", "-X", "DELETE",
			fmt.Sprintf("http://%s:%d/api/v1/images/%s",
				a.Config.MedaHost, a.Config.MedaPort, a.ImageName)}
	} else {
		// CLI call to delete image
		cmd = []string{a.Config.MedaBinary, "images", "rm", a.ImageName}
	}

	// Execute the command
	process := exec.Command(cmd[0], cmd[1:]...)
	err := process.Run()
	if err != nil {
		return fmt.Errorf("failed to destroy image %s: %w", a.ImageName, err)
	}

	return nil
}