package main

import (
	"context"
	"fmt"

	"golang.org/x/sync/errgroup"
	"k8s.io/apimachinery/pkg/api/errors"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	utilruntime "k8s.io/apimachinery/pkg/util/runtime"
	"k8s.io/client-go/kubernetes/scheme"
	"k8s.io/client-go/tools/cache"
	"k8s.io/client-go/util/workqueue"
	"k8s.io/klog/v2"

	testresourceapi "github.com/amurant/ring-go-operator/pkg/apis/testresource/v1"
	clientset "github.com/amurant/ring-go-operator/pkg/generated/clientset/versioned"
	samplescheme "github.com/amurant/ring-go-operator/pkg/generated/clientset/versioned/scheme"
	informers "github.com/amurant/ring-go-operator/pkg/generated/informers/externalversions/testresource/v1"
	listers "github.com/amurant/ring-go-operator/pkg/generated/listers/testresource/v1"
)

type Controller struct {
	resourceClientset clientset.Interface

	resourceLister listers.TestResourceLister
	resourceSynced cache.InformerSynced

	outNamespace string

	workqueue workqueue.RateLimitingInterface
}

func (c *Controller) enqueueResource(obj interface{}) error {
	if key, err := cache.MetaNamespaceKeyFunc(obj); err != nil {
		return err
	} else {
		c.workqueue.Add(key)
	}
	return nil
}

func NewController(
	resourceClientset clientset.Interface,
	resourceInformer informers.TestResourceInformer,
	outNamespace string,
) *Controller {
	utilruntime.Must(samplescheme.AddToScheme(scheme.Scheme))

	controller := &Controller{
		resourceClientset: resourceClientset,
		resourceLister:    resourceInformer.Lister(),
		resourceSynced:    resourceInformer.Informer().HasSynced,
		outNamespace:      outNamespace,
		workqueue:         workqueue.NewRateLimitingQueue(workqueue.DefaultControllerRateLimiter()),
	}

	klog.Info("Setting up event handlers")
	resourceInformer.Informer().AddEventHandler(cache.ResourceEventHandlerFuncs{
		AddFunc: func(new interface{}) {
			controller.enqueueResource(new)
		},
		UpdateFunc: func(old, new interface{}) {
			controller.enqueueResource(new)
		},
	})

	return controller
}

func (c *Controller) Run(ctx context.Context) error {
	defer c.workqueue.ShutDown()

	group, gctx := errgroup.WithContext(ctx)

	klog.Info("Waiting for informer caches to sync")
	if ok := cache.WaitForCacheSync(ctx.Done(), c.resourceSynced); !ok {
		return fmt.Errorf("failed to wait for caches to sync")
	}

	klog.Info("Reconciling")
	group.Go(func() error {
		for {
			obj, shutdown := c.workqueue.Get()

			if shutdown {
				return nil
			}

			if err := c.reconcile(gctx, obj); err != nil {
				utilruntime.HandleError(err)
			}
		}
	})

	group.Go(func() error {
		<-gctx.Done()

		c.workqueue.ShutDown()

		return nil
	})

	return group.Wait()
}

func (c *Controller) reconcile(ctx context.Context, obj interface{}) error {
	// We call Done here so the workqueue knows we have finished
	// processing this item. We also must remember to call Forget if we
	// do not want this work item being re-queued. For example, we do
	// not call Forget if a transient error occurs, instead the item is
	// put back on the workqueue and attempted again after a back-off
	// period.
	defer c.workqueue.Done(obj)
	var key string
	var ok bool

	// We expect strings to come off the workqueue. These are of the
	// form namespace/name. We do this as the delayed nature of the
	// workqueue means the items in the informer cache may actually be
	// more up to date that when the item was initially put onto the
	// workqueue.
	if key, ok = obj.(string); !ok {
		// As the item in the workqueue is actually invalid, we call
		// Forget here else we'd go into a loop of attempting to
		// process a work item that is invalid.
		c.workqueue.Forget(obj)
		return fmt.Errorf("expected string in workqueue but got %#v", obj)
	}

	// Run the syncHandler, passing it the namespace/name string of the
	// Foo resource to be synced.
	if err := c.syncHandler(ctx, key); err != nil {
		// Put the item back on the workqueue to handle any transient errors.
		c.workqueue.AddRateLimited(key)
		return fmt.Errorf("error syncing '%s': %s, requeuing", key, err.Error())
	}

	// Finally, if no error occurs we Forget this item so it does not
	// get queued again until another change happens.
	c.workqueue.Forget(obj)
	klog.Infof("Successfully synced '%s'", key)
	return nil
}

// syncHandler compares the actual state with the desired, and attempts to
// converge the two. It then updates the Status block of the Foo resource
// with the current status of the resource.
func (c *Controller) syncHandler(ctx context.Context, key string) error {
	// Convert the namespace/name string into a distinct namespace and name
	namespace, name, err := cache.SplitMetaNamespaceKey(key)
	if err != nil {
		return fmt.Errorf("invalid resource key: %s", key)
	}

	// Get the Test resource with this namespace/name
	inTestResource, err := c.resourceLister.TestResources(namespace).Get(name)
	if err != nil {
		// The Foo resource may no longer exist, in which case we stop
		// processing.
		if errors.IsNotFound(err) {
			return fmt.Errorf("IN testresource '%s' in work queue no longer exists", key)
		}

		return err
	}

	// Get the Test resource with this namespace/name
	outTestResource, err := c.resourceLister.TestResources(c.outNamespace).Get(name)
	if err != nil && !errors.IsNotFound(err) {
		return err
	}

	nowTimestamp := metav1.NowMicro()

	if err != nil && errors.IsNotFound(err) {
		klog.Infof("Creating resource")

		outTestResource = NewTestResource(name, c.outNamespace, inTestResource.Spec.Nonce, nowTimestamp)

		_, err := c.resourceClientset.AmurantV1().TestResources(c.outNamespace).Create(ctx, outTestResource, metav1.CreateOptions{})
		return err
	}

	if outTestResource.Spec.Nonce != inTestResource.Spec.Nonce {
		klog.Infof("Updating resource")

		outTestResource.Spec.Nonce = inTestResource.Spec.Nonce
		outTestResource.Spec.UpdatedAt = nowTimestamp

		_, err := c.resourceClientset.AmurantV1().TestResources(c.outNamespace).Update(ctx, outTestResource, metav1.UpdateOptions{})
		return err
	}

	return nil
}

func NewTestResource(
	name string,
	namespace string,
	nonce int64,
	startTimestamp metav1.MicroTime,
) *testresourceapi.TestResource {
	return &testresourceapi.TestResource{
		ObjectMeta: metav1.ObjectMeta{
			Name:      name,
			Namespace: namespace,
		},
		Spec: testresourceapi.TestResourceSpec{
			Nonce:     0,
			UpdatedAt: startTimestamp,
		},
	}
}
