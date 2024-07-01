use crate::{ json::VersionInfo, version_manager::remote::RawVersionList };

#[tokio::test]
async fn test_full_version_parsing() -> Result<(), Box<dyn std::error::Error>> {
  let version_list = RawVersionList::fetch().await?;
  for ver in version_list.versions {
    println!("Processing {}", ver.get_id());
    let ver = ver.fetch().await?;
    println!("{ver:#?}");
  }
  Ok(())
}
