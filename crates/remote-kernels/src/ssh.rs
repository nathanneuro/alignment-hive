use std::path::{Path, PathBuf};

use ssh_key::private::Ed25519Keypair;
use ssh_key::{LineEnding, PrivateKey};

pub struct SshKeypair {
    pub public_key_openssh: String,
    pub private_key_path: PathBuf,
}

/// Generate an ephemeral Ed25519 SSH keypair for pod access.
///
/// The private key is written to `.claude/remote-kernels/id_ed25519` in the project directory.
/// The public key is returned as an OpenSSH-format string for injection into the pod's env.
pub fn generate_keypair(project_dir: &Path) -> anyhow::Result<SshKeypair> {
    let dir = project_dir.join(".claude/remote-kernels");
    std::fs::create_dir_all(&dir)?;

    let private_key_path = dir.join("id_ed25519");

    let keypair = Ed25519Keypair::random(&mut rand::thread_rng());
    let private_key = PrivateKey::from(keypair);

    let private_pem = private_key.to_openssh(LineEnding::LF)?;
    std::fs::write(&private_key_path, private_pem.as_str())?;

    // Restrict permissions (owner read-only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&private_key_path, std::fs::Permissions::from_mode(0o600))?;
    }

    let public_key_openssh = private_key.public_key().to_openssh()?;

    tracing::info!(?private_key_path, "Generated ephemeral SSH keypair");

    Ok(SshKeypair {
        public_key_openssh,
        private_key_path,
    })
}
