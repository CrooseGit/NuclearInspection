use std::env::var_os;
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Points at the location of the .msg/.srv files
    let ros_share = var_os("ROS_SHARE")
        .map(PathBuf::from)
        .unwrap_or(PathBuf::from("/opt/ros/noetic/share"));
    let search_paths = vec![ros_share.join("std_msgs"), ros_share.join("geometry_msgs")];

    // Re-run the build script when the message source location changes.
    println!("cargo:rerun-if-env-changed=ROS_SHARE");

    // Actually invoke code generation on our search paths.
    let (source, dependent_paths) =
        roslibrust::codegen::find_and_generate_ros_messages_without_ros_package_path(search_paths)?;
    // This returns two things:
    // 1) A TokenStream which is the rust code we want to generate
    // 2) A list of paths that if modified would require the code to be regenerated. We use this to inform Cargo
    //    of when to re-run our build script.

    // It is important for build scripts to only output files to OUT_DIR which is an environment variable set by Cargo.
    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    // Name of the file in out_dir we want to write our generated code to
    let dest_path = std::path::Path::new(&out_dir).join("messages.rs");
    // Write the generated code to disk
    std::fs::write(dest_path, source.to_string())?;

    // If we stopped at this point, our code would still work, but Cargo would not know to rebuild
    // our package when a message file changed.
    // Cargo recognizes certain command line strings that build scripts print out:
    for path in &dependent_paths {
        // Tell cargo to re-run our build script if any of these files change
        println!("cargo:rerun-if-changed={}", path.display());
    }

    Ok(())
}
