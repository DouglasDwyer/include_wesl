import package::bar::baz;

@compute
@workgroup_size(1)
fn main(@builtin(workgroup_id) workgroup_id: vec3u) {
    @if(poogie)
    baz(workgroup_id.x);
}