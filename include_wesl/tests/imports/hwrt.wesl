/// A ray for hardware ray tracing.
@export struct HwRay {
    origin: vec3f,
    tmax: f32
}

fn cast_ray() -> u32 {
    @if(INNER)
    return 11u;
    @else
    return 12u;
}
