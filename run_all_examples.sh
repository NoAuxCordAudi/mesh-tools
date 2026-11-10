#!/bin/bash

# run_all_examples.sh
# Script to run all example files in the mesh-tools project
# Created: May 8, 2025

echo "=== Running all mesh-tools examples ==="
echo

# List of all examples in the project
EXAMPLES=(
  "animation_demo"
  "custom_mesh_demo"
  "discrete_normal_regions_demo"
  "emissive_mapping_demo"
  "hierarchy_demo"
  "instancing_demo"
  "materials_demo"
  "metalness_demo"
  "normal_mapping_demo"
  "occlusion_mapping_demo"
  "panel_grid_demo"
  "pbr_materials_demo"
  "primitives_demo"
  "roughness_demo"
  "rustacean_plate_demo"
  "simple_box"
  "skeletal_animation_demo"
  "specular_glossiness_demo"
  "terrain_demo"
  "texture_demo"
)

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Counters
TOTAL=0
PASSED=0
FAILED=0

# Run each example
for example in "${EXAMPLES[@]}"; do
  TOTAL=$((TOTAL+1))
  echo -e "${YELLOW}Running example:${NC} $example"
  
  # Run the example using cargo
  cargo run --example "$example"
  
  # Check if the example succeeded
  if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Successfully ran example:${NC} $example"
    echo -e "${GREEN}✓ Output file generated:${NC} ${example}.glb"
    PASSED=$((PASSED+1))
  else
    echo -e "${RED}✗ Failed to run example:${NC} $example"
    FAILED=$((FAILED+1))
  fi
  
  echo "----------------------------------------"
  echo
done

# Print summary
echo "=== Summary ==="
echo -e "Total examples: $TOTAL"
echo -e "Examples passed: ${GREEN}$PASSED${NC}"

if [ $FAILED -gt 0 ]; then
  echo -e "Examples failed: ${RED}$FAILED${NC}"
else
  echo -e "Examples failed: ${GREEN}$FAILED${NC}"
fi

# Check if all examples ran successfully
if [ $PASSED -eq $TOTAL ]; then
  echo -e "\n${GREEN}All examples ran successfully!${NC}"
else
  echo -e "\n${RED}Some examples failed to run. Please check the output above for details.${NC}"
  exit 1
fi

# List generated GLB files
echo -e "\n=== Generated GLB Files ==="
find "$(pwd)" -maxdepth 1 -name "*.glb" -printf "%f\n" | sort
