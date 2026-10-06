//! Picks the renderer before iced starts: the GPU (wgpu) when a real graphics
//! adapter exists, otherwise the CPU (tiny-skia).

use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renderer {
    Gpu,
    Cpu,
}

const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

pub fn select() -> Renderer {
    let choice = match std::env::var("SECBLITZ_RENDERER").ok().as_deref() {
        Some("cpu") => Choice::Cpu,
        Some("gpu") => Choice::Gpu(None),
        _ => probe_with_timeout(),
    };
    apply(choice)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Cpu,
    Gpu(Option<Api>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Api {
    Dx12,
    Vulkan,
}

fn apply(choice: Choice) -> Renderer {
    match choice {
        Choice::Cpu => {
            std::env::set_var("ICED_BACKEND", "tiny-skia");
            Renderer::Cpu
        }
        Choice::Gpu(api) => {
            if std::env::var_os("WGPU_BACKEND").is_none() {
                match api {
                    Some(Api::Dx12) => std::env::set_var("WGPU_BACKEND", "dx12"),
                    Some(Api::Vulkan) => std::env::set_var("WGPU_BACKEND", "vulkan"),
                    None => {}
                }
            }
            if std::env::var_os("WGPU_POWER_PREF").is_none() {
                std::env::set_var("WGPU_POWER_PREF", "low");
            }
            Renderer::Gpu
        }
    }
}

pub fn use_msaa(renderer: Renderer) -> bool {
    renderer == Renderer::Gpu
}

fn probe_with_timeout() -> Choice {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("gpu-probe".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(probe).unwrap_or(Choice::Cpu);
            let _ = tx.send(result);
        });
    if spawned.is_err() {
        return Choice::Cpu;
    }
    rx.recv_timeout(PROBE_TIMEOUT).unwrap_or(Choice::Cpu)
}

fn probe() -> Choice {
    let (dx12, vulkan) = adapters();
    match (dx12, vulkan) {
        (true, _) => Choice::Gpu(Some(Api::Dx12)),
        (false, true) => Choice::Gpu(Some(Api::Vulkan)),
        _ => Choice::Cpu,
    }
}

fn adapters() -> (bool, bool) {
    let backends = if cfg!(windows) {
        wgpu::Backends::DX12 | wgpu::Backends::VULKAN
    } else {
        wgpu::Backends::PRIMARY
    };
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });
    let (mut dx12, mut vulkan) = (false, false);
    for adapter in instance.enumerate_adapters(backends) {
        let info = adapter.get_info();
        if !is_real_gpu(&info.name, info.device_type) {
            continue;
        }
        match info.backend {
            wgpu::Backend::Dx12 => dx12 = true,
            wgpu::Backend::Vulkan => vulkan = true,
            _ => {}
        }
    }
    (dx12, vulkan)
}

pub fn is_real_gpu(name: &str, device_type: wgpu::DeviceType) -> bool {
    match device_type {
        wgpu::DeviceType::IntegratedGpu
        | wgpu::DeviceType::DiscreteGpu
        | wgpu::DeviceType::VirtualGpu => {}
        wgpu::DeviceType::Cpu | wgpu::DeviceType::Other => return false,
    }
    let lower = name.to_ascii_lowercase();
    const SOFTWARE: [&str; 6] = [
        "microsoft basic render",
        "warp",
        "llvmpipe",
        "swiftshader",
        "softpipe",
        "software",
    ];
    !SOFTWARE.iter().any(|s| lower.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::DeviceType as D;

    #[test]
    fn accepts_hardware_gpus() {
        assert!(is_real_gpu("NVIDIA GeForce RTX 4060", D::DiscreteGpu));
        assert!(is_real_gpu("Intel(R) UHD Graphics 620", D::IntegratedGpu));
        assert!(is_real_gpu("AMD Radeon(TM) Graphics", D::IntegratedGpu));
        assert!(is_real_gpu("Virtio GPU", D::VirtualGpu));
    }

    #[test]
    fn rejects_software_rasterizers() {
        assert!(!is_real_gpu("Microsoft Basic Render Driver", D::Cpu));
        assert!(!is_real_gpu(
            "Microsoft Basic Render Driver",
            D::IntegratedGpu
        ));
        assert!(!is_real_gpu("Microsoft Direct3D12 (WARP)", D::VirtualGpu));
        assert!(!is_real_gpu("llvmpipe (LLVM 15.0.7, 256 bits)", D::Cpu));
        assert!(!is_real_gpu("SwiftShader Device (Subzero)", D::DiscreteGpu));
        assert!(!is_real_gpu("Unknown", D::Other));
        assert!(!is_real_gpu("Anything", D::Cpu));
    }

    #[test]
    fn msaa_only_on_gpu() {
        assert!(use_msaa(Renderer::Gpu));
        assert!(!use_msaa(Renderer::Cpu));
    }

    #[test]
    fn apply_cpu_forces_tiny_skia() {
        assert_eq!(apply(Choice::Cpu), Renderer::Cpu);
        assert_eq!(std::env::var("ICED_BACKEND").unwrap(), "tiny-skia");
        std::env::remove_var("ICED_BACKEND");
    }
}
