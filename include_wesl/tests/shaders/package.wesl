import package::math;

@if(alpha)
fn alpha_enabled() { }

@compute
@workgroup_size(1)
fn main() {
    var x = 0u;

    @if(beta)
    x = 7u;
    @else
    x = 8u;

    x = math::offset(x);
}
