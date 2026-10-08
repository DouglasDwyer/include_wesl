import package::bar::baz;

@compute
@workgroup_size(1)
fn main(@builtin(workgroup_id) workgroup_id: vec3u) {
    @if(some_feature)
    bazz(workgroup_id.x);
}