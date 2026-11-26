//! The compute capability probe (#2620): can this device run our kernels,
//! and within which limits. WebGL2 and GL below ES 3.1 have no compute
//! shaders; the probe says "no compute" by name, and the caller runs its
//! CPU code. Provenance: our own rule over wgpu's public
//! `DownlevelFlags` and `Limits` fields.

/// Workgroup width of every kernel in this crate (`@workgroup_size(64)`).
pub const WORKGROUP: u32 = 64;

/// The limits a kernel plans against: read from the device, never assumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Compute {
    pub max_storage_buffer_binding_size: u64,
    pub max_buffer_size: u64,
    pub max_compute_workgroup_storage_size: u32,
    pub max_compute_workgroups_per_dimension: u32,
}

/// What the device can compute, or why it cannot. `backend` and
/// `downlevel` come from the adapter, `limits` from the device (the limits
/// that it was made with, which can be lower than the adapter's).
pub fn probe(
    backend: wgpu::Backend,
    downlevel: &wgpu::DownlevelCapabilities,
    limits: &wgpu::Limits,
) -> Result<Compute, String> {
    if !downlevel.flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS) {
        return Err(if backend == wgpu::Backend::Gl {
            "no compute: WebGL2 or GL below ES 3.1 (the Gl adapter has no compute shaders)".into()
        } else {
            format!("no compute: the {backend:?} adapter has no compute shaders")
        });
    }
    let need = [
        ("max_compute_workgroup_storage_size", u64::from(limits.max_compute_workgroup_storage_size), 1),
        ("max_compute_workgroups_per_dimension", u64::from(limits.max_compute_workgroups_per_dimension), 1),
        ("max_compute_invocations_per_workgroup", u64::from(limits.max_compute_invocations_per_workgroup), u64::from(WORKGROUP)),
        ("max_compute_workgroup_size_x", u64::from(limits.max_compute_workgroup_size_x), u64::from(WORKGROUP)),
        ("max_storage_buffers_per_shader_stage", u64::from(limits.max_storage_buffers_per_shader_stage), 2),
        ("max_storage_buffer_binding_size", limits.max_storage_buffer_binding_size, 4),
        ("max_buffer_size", limits.max_buffer_size, 4),
    ];
    for (name, have, min) in need {
        if have < min {
            return Err(format!("no compute: the device limit {name} is {have}, the kernels need {min}"));
        }
    }
    Ok(Compute {
        max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
        max_buffer_size: limits.max_buffer_size,
        max_compute_workgroup_storage_size: limits.max_compute_workgroup_storage_size,
        max_compute_workgroups_per_dimension: limits.max_compute_workgroups_per_dimension,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> wgpu::DownlevelCapabilities {
        wgpu::DownlevelCapabilities::default()
    }

    #[test]
    fn webgl2_like_downlevel_set_says_no_compute_by_name() {
        let gl = wgpu::DownlevelCapabilities { flags: wgpu::DownlevelFlags::empty(), ..full() };
        let lim = wgpu::Limits::downlevel_webgl2_defaults();
        let err = probe(wgpu::Backend::Gl, &gl, &lim).err().unwrap_or_default();
        assert!(err.contains("no compute") && err.contains("WebGL2"), "{err}");
    }

    #[test]
    fn missing_compute_flag_on_another_backend_names_the_backend() {
        let caps = wgpu::DownlevelCapabilities { flags: wgpu::DownlevelFlags::empty(), ..full() };
        let err = probe(wgpu::Backend::Vulkan, &caps, &wgpu::Limits::default()).err().unwrap_or_default();
        assert!(err.contains("no compute") && err.contains("Vulkan"), "{err}");
    }

    #[test]
    fn zero_workgroup_storage_is_no_compute() {
        let lim = wgpu::Limits { max_compute_workgroup_storage_size: 0, ..wgpu::Limits::default() };
        let err = probe(wgpu::Backend::Vulkan, &full(), &lim).err().unwrap_or_default();
        assert!(err.contains("max_compute_workgroup_storage_size"), "{err}");
    }

    #[test]
    fn default_limits_with_compute_pass_the_probe() {
        let c = probe(wgpu::Backend::Vulkan, &full(), &wgpu::Limits::default());
        let lim = wgpu::Limits::default();
        assert_eq!(c.map(|c| c.max_storage_buffer_binding_size), Ok(lim.max_storage_buffer_binding_size));
    }
}
