cmake_minimum_required(VERSION 3.0.2)
project(rust_nodes)

## Wrapper package for the Rust nodes.
##
## The nodes themselves live in the cargo workspace outside catkin_ws (see
## CARGO_WS below) this package only builds that workspace and symlinks each
## resulting binary to devel/lib/rust_nodes/ so that rosrun / roslaunch can
## find it with:
##
##     <node pkg="rust_nodes" type="<binary>"/>
##
## Adding a node: add one entry to RUST_NODES and a member to the workspace
## manifest (cargo_ws/Cargo.toml).

find_package(catkin REQUIRED)

catkin_package()

find_program(CARGO_BIN cargo
             HINTS /home/ubuntu/.cargo/bin "$ENV{HOME}/.cargo/bin")
if(NOT CARGO_BIN)
  message(FATAL_ERROR "cargo not found: install Rust or fix PATH")
endif()

set(CARGO_WS     /home/ubuntu/cargo_ws)
set(CARGO_TARGET ${CARGO_WS}/target)

set(RUST_NODES orchestrator)

message(STATUS "rust_nodes: cargo     = ${CARGO_BIN}")
message(STATUS "rust_nodes: workspace = ${CARGO_WS}")
message(STATUS "rust_nodes: binaries  = ${RUST_NODES}")

# Build every package in the cargo workspace
add_custom_target(rust_nodes_build ALL
  COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${CARGO_TARGET}
          ${CARGO_BIN} build --release --workspace --manifest-path ${CARGO_WS}/Cargo.toml
  COMMENT "Building Rust nodes: ${RUST_NODES}"
  VERBATIM)

foreach(node ${RUST_NODES})
  add_custom_command(TARGET rust_nodes_build POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E make_directory
            ${CATKIN_DEVEL_PREFIX}/${CATKIN_PACKAGE_BIN_DESTINATION}
    COMMAND ${CMAKE_COMMAND} -E create_symlink
            ${CARGO_TARGET}/release/${node}
            ${CATKIN_DEVEL_PREFIX}/${CATKIN_PACKAGE_BIN_DESTINATION}/${node}
    COMMENT "Linking rust node '${node}' into the devel space"
    VERBATIM)
endforeach()
